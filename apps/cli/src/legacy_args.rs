//! Accept the legacy ArmA3Sync command line forms and map them onto
//! subcommands:
//!
//! | legacy                               | new                                   |
//! |--------------------------------------|---------------------------------------|
//! | `-build <repo>`                      | `build --repo <repo>`                 |
//! | `-check <repo>`                      | `check-remote <repo>`                 |
//! | `-sync <repo> <dest> <true\|false>`  | `sync <repo> --dest <dest> [--exact]` |
//! | `-extract <src> <dst>`               | `extract-keys <src> <dst>`            |
//! | `-update`                            | (not supported: prints a message)     |

use std::ffi::OsString;

pub fn translate(args: &[OsString]) -> Result<Vec<OsString>, String> {
    let Some(first) = args.get(1).and_then(|a| a.to_str()) else {
        return Ok(args.to_vec());
    };
    let lower = first.to_ascii_lowercase();
    let rest: Vec<OsString> = args[2..].to_vec();
    let prog = args[0].clone();
    let s = |x: &str| OsString::from(x);
    let out = match lower.as_str() {
        "-build" if rest.len() == 1 => vec![prog, s("build"), s("--repo"), rest[0].clone()],
        "-check" if rest.len() == 1 => vec![prog, s("check-remote"), rest[0].clone()],
        "-extract" if rest.len() == 2 => vec![prog, s("extract-keys"), rest[0].clone(), rest[1].clone()],
        "-sync" if rest.len() == 3 => {
            let exact = match rest[2].to_str().map(|v| v.to_ascii_lowercase()) {
                Some(v) if v == "true" => true,
                Some(v) if v == "false" => false,
                _ => return Err("Unrecognized exact match parameter (true/false).".into()),
            };
            let mut v = vec![prog, s("sync"), rest[0].clone(), s("--dest"), rest[1].clone()];
            if exact {
                v.push(s("--exact"));
            }
            v
        }
        "-update" if rest.is_empty() => {
            return Err("ArmaXSync does not update itself; download new releases from the project page.".into())
        }
        "-console" | "-dev" | "-run" | "-safe" => {
            return Err(format!("{first}: legacy GUI/console modes are not supported by the CLI; use subcommands (see --help)."))
        }
        "-build" | "-check" | "-extract" | "-sync" => {
            return Err(format!(
                "ArmaXSync - bad command.\nUsage:\n  -build <repo>\n  -check <repo>\n  -sync <repo> <dest> <true|false>\n  -extract <src> <dst>\n(got {first} with {} argument(s))",
                rest.len()
            ))
        }
        _ => return Ok(args.to_vec()),
    };
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(a: &[&str]) -> Result<Vec<String>, String> {
        let v: Vec<OsString> = a.iter().map(OsString::from).collect();
        translate(&v).map(|r| r.into_iter().map(|x| x.into_string().unwrap()).collect())
    }

    #[test]
    fn maps_legacy_forms() {
        assert_eq!(
            t(&["x", "-BUILD", "My Repo"]).unwrap(),
            ["x", "build", "--repo", "My Repo"]
        );
        assert_eq!(
            t(&["x", "-sync", "R", "D:\\mods", "TRUE"]).unwrap(),
            ["x", "sync", "R", "--dest", "D:\\mods", "--exact"]
        );
        assert_eq!(
            t(&["x", "-sync", "R", "D", "false"]).unwrap(),
            ["x", "sync", "R", "--dest", "D"]
        );
        assert!(t(&["x", "-sync", "R", "D", "maybe"]).is_err());
        assert!(t(&["x", "-update"]).is_err());
        assert_eq!(t(&["x", "sync", "R"]).unwrap(), ["x", "sync", "R"]);
    }
}
