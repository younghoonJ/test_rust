use std::{env, fs, process};
use std::ffi::OsStr;
use std::fmt::{Debug};
use std::fs::{File, metadata};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

struct CompileTarget {
    vm_file: String,
    asm_file: String,
}

impl CompileTarget {
    fn new(parent_dir: &String, file_name: &String) -> CompileTarget {
        let vm_file = String::from(Path::new(parent_dir).join(file_name.to_owned() + ".vm").to_str().unwrap());
        let asm_file = String::from(Path::new(parent_dir).join(file_name.to_owned() + ".asm").to_str().unwrap());
        CompileTarget { vm_file, asm_file }
    }
}

fn parse_args(args: &[String]) -> Result<Vec<CompileTarget>, Box<dyn std::error::Error>> {
    fn f(path: &String, files: &mut Vec<CompileTarget>) {
        let path = Path::new(&path);
        let parent = String::from(path.parent().unwrap().to_str().unwrap());
        let file_stem = String::from(path.file_stem().unwrap().to_str().unwrap());

        match path.extension().and_then(OsStr::to_str) {
            Some("vm") => files.push(CompileTarget::new(&parent, &file_stem)),
            _ => ()
        }
    }

    if args.len() < 1 {
        return Err(String::from("not enough arguments").into());
    }

    let canonical_p = String::from(fs::canonicalize(PathBuf::from(&args[1]))?.as_path().to_str().unwrap());

    let mut files: Vec<CompileTarget> = Vec::new();

    if metadata(&canonical_p).unwrap().is_dir() {
        for path in fs::read_dir(&args[1]).unwrap() {
            let v = String::from(path.unwrap().path().as_path().to_str().unwrap());
            f(&v, &mut files);
        }
    } else if metadata(&canonical_p).unwrap().is_file() {
        let v = String::from(&canonical_p);
        f(&v, &mut files);
    }

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

    fn to_asm(&self, jump_count: u64, static_name: &str) {
        match &*self {
            VMCommand::CArithmetic(_, _) => println!("{:?}", self),
            VMCommand::CPush(seg, idx) => println!("{}", VMCommand::cpush2asm(seg, idx, jump_count, static_name).unwrap()),
            _ => println!("unmatched {:?}", self),
        }
    }

    fn cpush2asm(seg: &str, idx: &u16, jump_count: u64, static_name: &str) -> Result<String, String> {
        let s = match (seg, idx) {
            ("constant", idx) => format!("@{idx}\nD=A"),
            ("pointer", 0) => "@THIS\nD=M".to_string(),
            ("pointer", 1) => "@THAT\nD=M".to_string(),
            ("temp", idx) => format!("@5\nD=A\n@{idx}\nA=D+A\nD=M"),
            ("static", idx) => format!("@{static_name}.{idx}\nD=M"),
            (seg, idx) => {
                let mem_seg = match seg {
                    "local" => "LCL",
                    "argument" => "ARG",
                    "this" => "THIS",
                    "that" => "THAT",
                    _ => return Err(String::from(format!("unimplemented! {seg} {idx}"))),
                };
                format!("@{mem_seg}\nD=M\n@{idx}\nA=D+A\nD=M")
            }
        };
        Ok(s + "\n@SP\nA=M\nM=D\n@SP\nM=M+1\n")
    }
}

fn translate_vm(target_vm: &CompileTarget) -> Result<(), String> {
    println!("process {}.write to {}", &target_vm.vm_file, &target_vm.asm_file);
    let file = File::open(&target_vm.vm_file).unwrap();
    let reader = BufReader::new(file);

    for line in reader.lines() {
        let unwrapped = line.unwrap();
        let line_ = String::from(&unwrapped[..unwrapped.find("//").unwrap_or(unwrapped.len())]);
        if line_.len() > 0 {
            let x = VMCommand::new(&line_)?;
            x.to_asm(0, "0");
            println!("{:?}", x);
        }
    }
    Ok(())
}


fn main() {
    let args: Vec<String> = env::args().collect();
    let parsed = parse_args(&args).unwrap_or_else(|err| {
        println!("Problem parsing arguments: {}", err);
        process::exit(1);
    });

    for x in parsed {
        translate_vm(&x);
    }
}
