use std::{env, fs, process};
use std::ffi::OsStr;
use std::fmt::{Debug};
use std::fs::{File, metadata};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};



struct CompileTarget {
    vm_file: String,
    asm_file: String,
    static_name: String,
}

impl CompileTarget {
    fn new(parent_dir: &String, file_name: &String) -> CompileTarget {
        let static_name: String = file_name.to_string();
        let vm_file = Path::new(parent_dir).join(file_name.to_owned() + ".vm").to_str().unwrap().to_string();
        let asm_file = Path::new(parent_dir).join(file_name.to_owned() + ".asm").to_str().unwrap().to_string();

        CompileTarget { vm_file, asm_file, static_name }
    }
}

fn parse_args(args: &[String]) -> Result<Vec<CompileTarget>, Box<dyn std::error::Error>> {
    fn g(path: &String) -> CompileTarget {
        CompileTarget::new(&Path::new(&path).parent().unwrap().to_str().unwrap().to_string(),
                           &Path::new(&path).file_stem().unwrap().to_str().unwrap().to_string())
    }

    if args.len() < 1 {
        return Err("not enough arguments".to_string().into());
    }

    let canonical_p = fs::canonicalize(PathBuf::from(&args[1])).unwrap().as_path().to_str().unwrap().to_string();

    let files = if metadata(&canonical_p).unwrap().is_dir() {
        fs::read_dir(&canonical_p).unwrap()
            .map(|item| item.unwrap().path())
            .filter(|item| item.is_file() && item.extension().unwrap() == "vm")
            .map(|path| g(&path.to_str().unwrap().to_string())).collect()
    } else {
        vec![g(&String::from(&canonical_p))]
    };

    Ok(files)
}


#[derive(Debug)]
enum VMCommand {
    CArithmetic(String, u16),
    CPush(String, u16),
    CPop(String, u16),
    CLabel(String, u16),
    CGoto(String, u16),
    CIf(String, u16),
    CFunction(String, u16),
    CReturn(String, u16),
    CCall(String, u16),
}

fn get_mem_seg(seg: &str) -> Result<&str, String> {
    Ok(match seg {
        "local" => "LCL",
        "argument" => "ARG",
        "this" => "THIS",
        "that" => "THAT",
        seg => return Err(format!("unimplemented mem_seg: {seg}").to_string()),
    })
}

fn get_bi_op(comm: &str) -> Result<&str, String> {
    Ok(match comm {
        "add" => "+",
        "sub" => "-",
        "and" => "&",
        "or" => "|",
        comm => return Err(format!("unimplemented bi_op: {comm}").to_string()),
    })
}

fn get_si_op(comm: &str) -> Result<&str, String> {
    Ok(match comm {
        "neg" => "-",
        "not" => "!",
        comm => return Err(format!("unimplemented si_op: {comm}").to_string()),
    })
}

fn get_cmp_op(comm: &str) -> Result<&str, String> {
    Ok(match comm {
        "eq" => "JEQ",
        "gt" => "JGT",
        "lt" => "JLT",
        comm => return Err(format!("unimplemented cmp_op: {comm}").to_string()),
    })
}


impl VMCommand {
    fn new(s: &String) -> Result<VMCommand, String> {
        let v: Vec<&str> = s.as_str().trim().split(" ").collect();
        match v[0] {
            "push" => return Ok(VMCommand::CPush(v[1].trim().to_string(), v[2].trim().parse::<u16>().unwrap())),
            "pop" => return Ok(VMCommand::CPop(v[1].trim().to_string(), v[2].trim().parse::<u16>().unwrap())),
            "sub" | "add" | "and" | "or" | "neg" | "not" | "eq" | "gt" | "lt" =>
                Ok(VMCommand::CArithmetic(v[0].to_string(), 0)),
            s => return Err("unimplemented vm command type: ".to_string() + s),
        }
    }

    fn to_asm(&self, jump_count: &mut u64, static_name: &str) -> Result<(u64, String), String> {
        Ok(match &*self {
            VMCommand::CArithmetic(seg, _) => VMCommand::carithmetic2asm(seg, jump_count)?,
            VMCommand::CPush(seg, idx) => VMCommand::cpush2asm(seg, idx, static_name)?,
            VMCommand::CPop(seg, idx) => VMCommand::cpop2asm(seg, idx, static_name)?,
            _ => return Err("unmatched vmcommand ".to_string()),
        })
    }

    fn cpush2asm(seg: &str, idx: &u16, static_name: &str) -> Result<String, String> {
        let s = match (seg, idx) {
            ("constant", idx) => format!("@{idx}\nD=A"),
            ("pointer", 0) => "@THIS\nD=M".to_string(),
            ("pointer", 1) => "@THAT\nD=M".to_string(),
            ("temp", idx) => format!("@5\nD=A\n@{idx}\nA=D+A\nD=M"),
            ("static", idx) => format!("@{static_name}.{idx}\nD=M"),
            (seg, idx) => {
                let mem_seg = get_mem_seg(seg)?;
                format!("@{mem_seg}\nD=M\n@{idx}\nA=D+A\nD=M")
            }
        };
        Ok(s + "\n@SP\nA=M\nM=D\n@SP\nM=M+1")
    }

    fn cpop2asm(seg: &str, idx: &u16, static_name: &str) -> Result<String, String> {
        let s = match (seg, idx) {
            ("pointer", 0) => "@THIS\nD=A".to_string(),
            ("pointer", 1) => "@THAT\nD=A".to_string(),
            ("temp", idx) => format!("@5\nD=A\n@{idx}\nD=D+A"),
            ("static", idx) => format!("@{static_name}.{idx}\nD=A"),
            (seg, idx) => {
                let mem_seg = get_mem_seg(seg)?;
                format!("@{mem_seg}\nD=M\n@{idx}\nD=D+A")
            }
        };
        Ok(s + "\n@R15\nM=D\n@SP\nAM=M-1\nD=M\n@R15\nA=M\nM=D")
    }

    fn carithmetic2asm(comm: &str, jump_count: &mut u64) -> Result<String, String> {
        Ok(match comm {
            "neg" | "not" => {
                let op = get_si_op(comm)?;
                format!("@SP\nAM=M-1\nMD={op}M\n@SP\nM=M+1")
            }
            "add" | "sub" | "and" | "or" => {
                let op = get_bi_op(comm)?;
                format!("@SP\nAM=M-1\nD=M\nA=A-1\nMD=M{op}D")
            }
            "eq" | "gt" | "lt" => {
                let op = get_cmp_op(comm)?;
                let v = format!("@R15\nM=-1\n@SP\nAM=M-1\nD=M\nA=A-1\nD=M-D\n@JMP_FALSE{jump_count}\nD;{op}\n\
                @R15\nM=0\n(JMP_FALSE{jump_count})\n@R15\nD=M\n@SP\nA=M-1\nM=D");
                *jump_count += 1;
                v
            }
            comm => return Err(format!("unimplemented arithmetic: {comm}").to_string()),
        })
    }
}

fn translate_vm(target_vm: &CompileTarget) -> Result<String, String> {
    println!("process {} write to {}", &target_vm.vm_file, &target_vm.asm_file);
    let file = File::open(&target_vm.vm_file).unwrap();
    let reader = BufReader::new(file);

    let mut jump_count: u64 = 0;

    let mut result_asm = String::new();
    for line in reader.lines() {
        let unwrapped = line.unwrap();
        let line_ = &unwrapped[..unwrapped.find("//").unwrap_or(unwrapped.len())].to_string();
        if line_.len() > 0 {
            let vm_ = VMCommand::new(&line_)?;
            let asm_ = vm_.to_asm(&mut jump_count, target_vm.static_name.as_str())?;
            result_asm += &(asm_ + "\n");
        }
    }
    Ok(result_asm)
}


fn main() {
    let args: Vec<String> = env::args().collect();
    let input_vms = parse_args(&args).expect("Problem parsing arguments");
    for x in input_vms {
        let result_asm = translate_vm(&x).expect("failed to process vm file");
        let mut file = File::create(&x.asm_file).expect("failed to create an asm file!");
        file.write_all(result_asm.as_ref()).expect("failed to write asm file!");
    }
}
