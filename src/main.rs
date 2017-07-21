use std::env;

fn translate_path(arg: String) -> String {
    if let Some(index) = arg.find(":\\") {
        if index != 1 {
            println!("Not a path: {:?}", arg);
            return arg;
        }
        let mut path_chars = arg.chars();
        if let Some(drive) = path_chars.next() {
            let mut wsl_path = String::from("/mnt/");
            wsl_path.push_str(&drive.to_lowercase().collect::<String>());
            path_chars.next();
            wsl_path.push_str(&path_chars.map(|c|
                    match c {
                        '\\' => '/',
                        _ => c,
                    }
                ).collect::<String>());
            return wsl_path;
        }
    }
    arg
}

fn main() {
    // ToDo: Add git command as first item
    let git_args: Vec<String> = env::args().skip(1).map(
            translate_path).collect();
    for arg in git_args {
        println!("{:?}", arg);
    }
}
