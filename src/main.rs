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
        let vm_file = String::from(Path::new(parent_dir).join(file_name.to_owned() + ".vm").to_str().unwrap());
        let asm_file = String::from(Path::new(parent_dir).join(file_name.to_owned() + ".asm").to_str().unwrap());

        CompileTarget { vm_file, asm_file, static_name }
    }
}

fn parse_args(args: &[String]) -> Result<Vec<CompileTarget>, Box<dyn std::error::Error>> {
    fn g(path: &String) -> CompileTarget {
        let path = Path::new(&path);
        let parent = String::from(path.parent().unwrap().to_str().unwrap());
        let file_stem = String::from(path.file_stem().unwrap().to_str().unwrap());
        CompileTarget::new(&parent, &file_stem)
    }

    if args.len() < 1 {
        return Err(String::from("not enough arguments").into());
    }

    let canonical_p = String::from(fs::canonicalize(PathBuf::from(&args[1]))?.as_path().to_str().unwrap());

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
        seg => return Err(String::from(format!("unimplemented mem_seg: {seg}"))),
    })
}

fn get_bi_op(comm: &str) -> Result<&str, String> {
    Ok(match comm {
        "add" => "+",
        "sub" => "-",
        "and" => "&",
        "or" => "|",
        comm => return Err(String::from(format!("unimplemented bi_op: {comm}"))),
    })
}

fn get_si_op(comm: &str) -> Result<&str, String> {
    Ok(match comm {
        "neg" => "-",
        "not" => "!",
        comm => return Err(String::from(format!("unimplemented si_op: {comm}"))),
    })
}

fn get_cmp_op(comm: &str) -> Result<&str, String> {
    Ok(match comm {
        "eq" => "JEQ",
        "gt" => "JGT",
        "lt" => "JLT",
        comm => return Err(String::from(format!("unimplemented cmp_op: {comm}"))),
    })
}


impl VMCommand {
    fn new(s: &String) -> Result<VMCommand, String> {
        let v: Vec<&str> = s.as_str().trim().split(" ").collect();
        match v[0] {
            "push" => return Ok(VMCommand::CPush(String::from(v[1].trim()), v[2].trim().parse::<u16>().unwrap())),
            "pop" => return Ok(VMCommand::CPop(String::from(v[1].trim()), v[2].trim().parse::<u16>().unwrap())),
            "sub" | "add" | "and" | "or" | "neg" | "not" | "eq" | "gt" | "lt" =>
                Ok(VMCommand::CArithmetic(String::from(v[0]), 0)),
            s => return Err(String::from("unimplemented vm command type: ") + s),
        }
    }

    fn to_asm(&self, jump_count: &mut u64, static_name: &str) -> Result<String, String> {
        Ok(match &*self {
            VMCommand::CArithmetic(seg, _) => VMCommand::carithmetic2asm(seg, jump_count)?,
            VMCommand::CPush(seg, idx) => VMCommand::cpush2asm(seg, idx, static_name)?,
            VMCommand::CPop(seg, idx) => VMCommand::cpop2asm(seg, idx, static_name)?,
            _ => return Err(String::from("unmatched vmcommand ")),
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
            ("pointer", 0) => "@THISD=A".to_string(),
            ("pointer", 1) => "@THATD=A".to_string(),
            ("temp", idx) => format!("@5\nD=A\n@{idx}\nD=D+A"),
            ("static", idx) => format!("@{static_name}.{idx}\nD=A"),
            (seg, idx) => {
                let mem_seg = get_mem_seg(seg)?;
                format!("@{mem_seg}\nD=M\n@{idx}\nD=D+A")
            }
        };
        Ok(s + "\n@R15\nM=D\n@SP\nAM=M-1\nD=M\nR15\nA=M\nM=D")
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
                format!("@R15\nM=-1\n@SP\nAM=M-1\nD=M\nA=A-1\nD=M-D\n@JMP_FALSE{jump_count}\nD;{op}\n\
                @R15\nM=0\n(JMP_FALSE{jump_count})\n@R15\nD=M\n@SP\nA=M-1\nM=D")
            }
            comm => return Err(String::from(format!("unimplemented arithmetic: {comm}"))),
        })
    }
}

fn translate_vm(target_vm: &CompileTarget) -> Result<String, String> {
    println!("process {}.write to {}", &target_vm.vm_file, &target_vm.asm_file);
    let file = File::open(&target_vm.vm_file).unwrap();
    let reader = BufReader::new(file);

    let mut jump_count: u64 = 0;

    let mut result_asm = String::new();
    for line in reader.lines() {
        let unwrapped = line.unwrap();
        let line_ = String::from(&unwrapped[..unwrapped.find("//").unwrap_or(unwrapped.len())]);
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
    let input_vms = parse_args(&args).unwrap_or_else(|err| {
        println!("Problem parsing arguments: {}", err);
        process::exit(1);
    });

    for x in input_vms {
        let result_asm = translate_vm(&x).unwrap();
        print!("{}", result_asm);
        let mut file = File::create(&x.asm_file).expect("failed to create an asm file!");
        file.write_all(result_asm.as_ref()).expect("failed to write asm file!");
    }
}
