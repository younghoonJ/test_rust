use std::{env, fs};
use std::ffi::OsStr;
use std::fs::metadata;
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

fn parse_args(args: &[String]) -> Vec<CompileTarget> {
    let mut files: Vec<CompileTarget> = Vec::new();

    fn f(path: &String, files: &mut Vec<CompileTarget>) {
        let path = Path::new(&path);
        let parent = String::from(path.parent().unwrap().to_str().unwrap());
        let file_stem = String::from(path.file_stem().unwrap().to_str().unwrap());

        match path.extension().and_then(OsStr::to_str) {
            Some("vm") => files.push(CompileTarget::new(&parent, &file_stem)),
            _ => ()
        }
    }

    let src_path = PathBuf::from(&args[1]);
    let canonical_p = String::from(fs::canonicalize(&src_path).unwrap().as_path().to_str().unwrap());

    if metadata(&canonical_p).unwrap().is_dir() {
        for path in fs::read_dir(&args[1]).unwrap() {
            let v = String::from(path.unwrap().path().as_path().to_str().unwrap());
            f(&v, &mut files);
        }
    } else if metadata(&canonical_p).unwrap().is_file() {
        let v = String::from(&canonical_p);
        f(&v, &mut files);
    }

    files
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let parsed = parse_args(&args);
    for x in parsed {
        println!("{}, {} ", x.vm_file, x.asm_file);
    }
}
