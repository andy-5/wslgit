use std::env;

fn share_key(key: &str, translate_path: bool) {
    let wslenv_key = if translate_path {
        format!("{}/p", key)
    } else {
        key.to_owned()
    };

    let original_wslenv = env::var("WSLENV").unwrap_or_default();
    let mut entries = Vec::new();
    let mut key_added = false;

    for entry in original_wslenv.split(':').filter(|entry| !entry.is_empty()) {
        let entry_key = entry.split('/').next().unwrap();
        if entry_key.eq_ignore_ascii_case(key) {
            if !key_added {
                entries.push(wslenv_key.clone());
                key_added = true;
            }
        } else {
            entries.push(entry.to_owned());
        }
    }

    if !key_added {
        entries.push(wslenv_key);
    }

    env::set_var("WSLENV", entries.join(":"));
}

/// Share a value to WSL by using an environment variable and `WSLENV`.
///
/// * `key` - Name to use for the environment variable.
/// * `value` - The value of the environment variable.
/// * `translate_path` - If `true` will append `/p` to the variable name when added to `WSLENV`.
pub fn share_val(key: &str, value: &str, translate_path: bool) {
    env::set_var(key, value);
    share_key(key, translate_path);
}

/// Share an existing environment variable with WSL when it is present.
pub fn share_env(key: &str, translate_path: bool) {
    if env::var_os(key).is_some() {
        share_key(key, translate_path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn share_variable_to_wsl() {
        // No WSLENV
        env::remove_var("WSLENV");
        share_val("VAR1", "1", false);
        assert_eq!("1", env::var("VAR1").unwrap());
        assert_eq!("VAR1", env::var("WSLENV").unwrap());

        // Empty WSLENV
        env::set_var("WSLENV", "");
        share_val("VAR2", "2", false);
        assert_eq!("2", env::var("VAR2").unwrap());
        assert_eq!("VAR2", env::var("WSLENV").unwrap());

        // Non-empty WSLENV
        env::set_var("WSLENV", "A");
        share_val("VAR3", "3", false);
        assert_eq!("3", env::var("VAR3").unwrap());
        assert_eq!("A:VAR3", env::var("WSLENV").unwrap());

        // Variable exists and already in WSLENV
        env::set_var("VAR4", "0");
        env::set_var("WSLENV", "VAR1:VAR2:VAR3:VAR4:VAR5");
        share_val("VAR4", "4", false);
        assert_eq!("4", env::var("VAR4").unwrap());
        assert_eq!("VAR1:VAR2:VAR3:VAR4:VAR5", env::var("WSLENV").unwrap());

        // Variable exists and already in WSLENV but without /p flag
        env::set_var("VAR5", "0");
        env::set_var("WSLENV", "VAR1:VAR2:VAR3:VAR4:VAR5");
        share_val("VAR5", "5", true);
        assert_eq!("5", env::var("VAR5").unwrap());
        assert_eq!("VAR1:VAR2:VAR3:VAR4:VAR5/p", env::var("WSLENV").unwrap());
    }
}
