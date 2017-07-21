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

fn shell_escape(arg: String) -> String {
    // ToDo: This really only handles arguments with spaces.
    // More complete shell escaping is required for the general case.
    if arg.contains(" ") {
        return vec![
            String::from("\""),
            arg,
            String::from("\"")].join("");
    }
    arg
}

fn main() {
    // ToDo: Add git command as first item
    let mut git_args: Vec<String> = vec![String::from("git")];
    git_args.extend(env::args().skip(1)
        .map(translate_path)
        .map(shell_escape));
    let git_cmd = git_args.join(" ");
    println!("{}", git_cmd);
}
