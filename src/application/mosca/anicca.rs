// use core::panic;
// use dirs;
// use serde::Deserialize;
// use std::{
//     collections::HashMap,
//     fs,
//     io::{Error, ErrorKind},
// };

// #[derive(Deserialize, Debug)]
// struct Config {
//     nicca: Nicca,
// }

// #[derive(Deserialize, Debug)]
// struct Nicca {
//     list: Vec<String>,
// }

// pub fn anicca() -> Result<(), Error> {
//     let config_dir = dirs::config_dir().unwrap();
//     let config_path = String::from(config_dir.to_str().unwrap()) + "/lince/os.toml";
//     let config_contents: Config = toml::from_str(&fs::read_to_string(config_path)?).unwrap();

//     let persist_list = &config_contents.nicca.list;

//     let mut map: HashMap<String, Vec<String>> = HashMap::new();

//     for line in persist_list {
//         let split_line = line.split("/");
//         let last_part = split_line.clone().last().unwrap();

//         let mut joined_line = String::new();

//         for part in split_line {
//             if part != last_part {
//                 joined_line.push_str(part);
//                 joined_line.push_str("/");
//             }
//         }
//         map.entry(joined_line)
//             .or_default()
//             .push(last_part.to_string());
//     }
//     for (key, _values) in &map {
//         let dir = fs::read_dir(key)?;

//         for entry in dir {
//             if entry.is_err() {
//                 panic!(
//                     "Entry: {entry:?} caused the program to panic due to an error: {}",
//                     entry.as_ref().unwrap_err()
//                 )
//             }

//             let path = entry.unwrap().path();

//             let path = path.to_string_lossy();

//             if !persist_list.contains(&path.to_string()) {
//                 let _ = match fs::remove_dir_all(&*path) {
//                     Ok(file) => file,
//                     Err(error) => match error.kind() {
//                         ErrorKind::NotADirectory => match fs::remove_file(path.to_string()) {
//                             Ok(fd) => fd,
//                             Err(e) => panic!("Problem deleting file: {e:?}"),
//                         },
//                         ErrorKind::NotFound => println!("Error not found hayaa: {path}"),
//                         other_error => {
//                             panic!("Other problem: {other_error:?}");
//                         }
//                     },
//                 };
//             }
//         }
//     }

//     Ok(())
// }
