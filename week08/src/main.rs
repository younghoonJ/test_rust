use std::{env, fs};
use std::fmt::{Debug};
use std::fs::{File, metadata};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

struct CompileTarget {
    vm_file: String,
    static_name: String,
}

impl CompileTarget {
    fn new(parent_dir: &String, file_name: &String) -> CompileTarget {
        let static_name: String = file_name.to_string();
        let vm_file = Path::new(parent_dir).join(file_name.to_owned() + ".vm").to_str().unwrap().to_string();

        CompileTarget { vm_file, static_name }
    }
}

fn parse_args(args: &[String]) -> Result<(Vec<CompileTarget>, String, bool), Box<dyn std::error::Error>> {
    fn g(path: &String) -> CompileTarget {
        CompileTarget::new(&Path::new(&path).parent().unwrap().to_str().unwrap().to_string(),
                           &Path::new(&path).file_stem().unwrap().to_str().unwrap().to_string())
    }

    if args.len() < 1 {
        return Err("not enough arguments".to_string().into());
    }

    let pbf = PathBuf::from(&args[1]);
    let canonical_p = fs::canonicalize(&pbf).unwrap()
        .as_path().to_str().unwrap().to_string();

    let files = if metadata(&canonical_p).unwrap().is_dir() {
        let asm_out = Path::new(&canonical_p).join(pbf.file_stem().unwrap().to_str().unwrap().to_string() + ".asm").to_str().unwrap().to_string();
        (fs::read_dir(&canonical_p).unwrap()
             .map(|item| item.unwrap().path())
             .filter(|item| item.is_file() && item.extension().unwrap() == "vm")
             .map(|path| g(&path.to_str().unwrap().to_string())).collect(),
         asm_out,
         true)
    } else {
        let asm_out = Path::new(&canonical_p).parent().unwrap().join(pbf.file_stem().unwrap().to_str().unwrap().to_string() + ".asm").to_str().unwrap().to_string();
        (vec![g(&String::from(&canonical_p))],
         asm_out,
         false)
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
        Ok(match v[0] {
            "push" => VMCommand::CPush(v[1].trim().to_string(), v[2].trim().parse::<u16>().unwrap()),
            "pop" => VMCommand::CPop(v[1].trim().to_string(), v[2].trim().parse::<u16>().unwrap()),
            "sub" | "add" | "and" | "or" | "neg" | "not" | "eq" | "gt" | "lt" =>
                VMCommand::CArithmetic(v[0].to_string(), 0),
            "label" => VMCommand::CLabel(v[1].trim().to_string(), 0),
            "goto" => VMCommand::CGoto(v[1].trim().to_string(), 0),
            "if-goto" => VMCommand::CIf(v[1].trim().to_string(), 0),
            "function" => VMCommand::CFunction(v[1].trim().to_string(), v[2].trim().parse::<u16>().unwrap()),
            "return" => VMCommand::CReturn(v[0].trim().to_string(), 0),
            "call" => VMCommand::CCall(v[1].trim().to_string(), v[2].trim().parse::<u16>().unwrap()),
            s => return Err("unimplemented vm command type: ".to_string() + s),
        })
    }

    fn to_asm(&self, jump_count: u64, static_name: &str, curr_fname: &str)
              -> Result<(u64, String), String> {
        Ok(match &*self {
            VMCommand::CArithmetic(seg, _) => VMCommand::carithmetic2asm(seg, jump_count)?,
            VMCommand::CPush(seg, idx) => (jump_count, VMCommand::cpush2asm(seg, idx, static_name)?),
            VMCommand::CPop(seg, idx) => (jump_count, VMCommand::cpop2asm(seg, idx, static_name)?),
            VMCommand::CLabel(seg, _) => (jump_count, VMCommand::clabel2asm(seg, curr_fname)?),
            VMCommand::CGoto(seg, _) => (jump_count, VMCommand::cgoto2asm(seg, curr_fname)?),
            VMCommand::CIf(seg, _) => (jump_count, VMCommand::cif2asm(seg, curr_fname)?),
            VMCommand::CFunction(fname, n_args) => (jump_count, VMCommand::cfunction2asm(fname, n_args)?),
            VMCommand::CReturn(_, _) => (jump_count, VMCommand::creturn2asm()?),
            VMCommand::CCall(fname, n_args) => VMCommand::ccall2asm(fname, n_args, jump_count)?,
        })
    }

    fn ccall2asm(fname: &String, n_args: &u16, jump_count: u64) -> Result<(u64, String), String> {
        let return_addr = format!("{fname}$ret.{jump_count}");
        let asm_stk_push = "@SP\nAM=M+1\nA=A-1\nM=D";
        let ret_ = format!(
            "@{return_addr}\nD=A\n{asm_stk_push}\n\
            @LCL\nD=M\n{asm_stk_push}\n\
            @ARG\nD=M\n{asm_stk_push}\n\
            @THIS\nD=M\n{asm_stk_push}\n\
            @THAT\nD=M\n{asm_stk_push}\n\
            @SP\nD=M\n@LCL\nM=D\n\
            @5\nD=D-A\n@{n_args}\nD=D-A\n@ARG\nM=D\n\
            @{fname}\n0;JMP\n\
            ({return_addr})"
        );
        Ok((jump_count + 1, ret_))
    }

    fn creturn2asm() -> Result<String, String> {
        // frame = "R13", ret_addr = "R14"
        // frame = LCL
        // retAddr = *(frame - 5)
        // *ARG = pop()
        // SP = ARG + 1
        // THAT = *(frame - 1)
        // THIS = *(frame - 2)
        // ARG = *(frame - 3)
        // LCL = *(frame - 4)
        Ok("\
            @LCL\nD=M\n@R13\nM=D\n\
            @5\nA=D-A\nD=M\n@R14\nM=D\n\
            @SP\nAM=M-1\nD=M\n@ARG\nA=M\nM=D\n\
            @ARG\nD=M\n@SP\nM=D+1\n\
            @R13\nAM=M-1\nD=M\n@THAT\nM=D\n\
            @R13\nAM=M-1\nD=M\n@THIS\nM=D\n\
            @R13\nAM=M-1\nD=M\n@ARG\nM=D\n\
            @R13\nAM=M-1\nD=M\n@LCL\nM=D\n\
            @R14\nA=M\n0;JMP".to_string())
    }

    fn cfunction2asm(func_name: &str, n_vars: &u16) -> Result<String, String> {
        let zu16_0: u16 = 0;
        Ok(match n_vars {
            v if *v == zu16_0 => { format!("({func_name})") }
            v if *v > zu16_0 => {
                format!(
                    "({func_name})\n\
                    @{n_vars}\n\
                    D=A\n\
                    ({func_name}_rep)\n\
                    @SP\n\
                    AM=M+1\n\
                    A=A-1\n\
                    M=0\n\
                    @{func_name}_rep\n\
                    D=D-1;JGT"
                )
            }
            _ => return Err("number of argument cannot be less than zero.".to_string()),
        })
    }

    fn cgoto2asm(label: &str, func_name: &str) -> Result<String, String> {
        Ok(format!("@{func_name}${label}\n0;JMP"))
    }

    fn cif2asm(label: &str, func_name: &str) -> Result<String, String> {
        Ok(format!(
            "@SP\n\
            AM=M-1\n\
            D=M\n\
            @{func_name}${label}\n\
            D;JNE"))
    }

    fn clabel2asm(label: &str, func_name: &str) -> Result<String, String> {
        Ok(format!("({func_name}${label})"))
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
        Ok(s + "\n@SP\nAM=M+1\nA=A-1\nM=D")
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
        Ok(s + "\n\
            @R15\n\
            M=D\n\
            @SP\n\
            AM=M-1\n\
            D=M\n\
            @R15\n\
            A=M\n\
            M=D")
    }

    fn carithmetic2asm(comm: &str, jump_count: u64) -> Result<(u64, String), String> {
        Ok(match comm {
            "neg" | "not" => {
                let op = get_si_op(comm)?;
                (jump_count, format!(
                    "@SP\n\
                    AM=M-1\n\
                    MD={op}M\n\
                    @SP\n\
                    M=M+1"
                ))
            }
            "add" | "sub" | "and" | "or" => {
                let op = get_bi_op(comm)?;
                (jump_count, format!(
                    "@SP\n\
                    AM=M-1\n\
                    D=M\n\
                    A=A-1\n\
                    MD=M{op}D"
                ))
            }
            "eq" | "gt" | "lt" => {
                let op = get_cmp_op(comm)?;
                let v = format!(
                    "@R15\n\
                    M=-1\n\
                    @SP\n\
                    AM=M-1\n\
                    D=M\n\
                    A=A-1\n\
                    D=M-D\n\
                    @JMP_FALSE{jump_count}\n\
                    D;{op}\n\
                    @R15\n\
                    M=0\n\
                    (JMP_FALSE{jump_count})\n\
                    @R15\n\
                    D=M\n\
                    @SP\n\
                    A=M-1\n\
                    M=D");
                (jump_count + 1, v)
            }
            comm => return Err(format!("unimplemented arithmetic: {comm}").to_string()),
        })
    }
}

fn translate_vm(target_vm: &CompileTarget, asm_out: &str, jump_count: u64) -> Result<(String, u64), String> {
    println!("process {} write to {}", &target_vm.vm_file, asm_out);
    let file = File::open(&target_vm.vm_file).unwrap();
    let reader = BufReader::new(file);

    let mut jump_count: u64 = jump_count;
    let mut func_name = "System".to_string();

    let mut result_asm = String::new();
    for line in reader.lines() {
        let unwrapped = line.unwrap();
        let line_ = &unwrapped[..unwrapped.find("//")
            .unwrap_or(unwrapped.len())].to_string();
        if line_.len() > 0 {
            let vm_ = VMCommand::new(&line_)?;
            let ret_ = vm_.to_asm(jump_count, target_vm.static_name.as_str(), func_name.as_str())?;
            jump_count = ret_.0;
            result_asm += &(ret_.1 + "\n");
            if let VMCommand::CFunction(fname, _) = vm_ {
                func_name = fname.to_string();
            }
        }
    }
    Ok((result_asm, jump_count))
}

fn bootstrap(jump_cnt: u64) -> (String, u64) {
    let zu16: u16 = 0;
    let stack_base_addr = 256;
    let cmd = format!("@{stack_base_addr}\nD=A\n@SP\nM=D\n");
    let ccall_ = VMCommand::ccall2asm(&"Sys.init".to_string(), &zu16, 0).unwrap().1 + "\n";
    (cmd + ccall_.as_str(), jump_cnt + 1)
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let input_vms = parse_args(&args).expect("Problem parsing arguments");
    let asm_out = input_vms.1.as_str();
    let mut file = File::create(asm_out).expect("failed to create an asm file!");

    let mut jump_count = 0;
    if input_vms.2 {
        file.write_all(bootstrap(jump_count).0.as_ref()).expect("failed to write asm file!");
    }
    for x in input_vms.0 {
        let result_asm = translate_vm(&x, asm_out, jump_count)
            .expect("failed to process vm file");
        jump_count = result_asm.1;
        file.write_all(result_asm.0.as_ref()).expect("failed to write asm file!");
    }
}
