//! Generate the site content.

// Standard library
use std::fs::{File, OpenOptions};
use std::io::prelude::*;
use std::path::{Path, PathBuf};

// Third party
use glob::glob;
use pandoc::{Pandoc, PandocOption, PandocOutput, InputFormat, OutputFormat, OutputKind};
use toml;

// First party
use syntax_highlighting::syntax_highlight;


pub struct Site {
    pub source_directory: PathBuf,
}


/// Generate content from a configuration.
pub fn generate(site: Site) -> Result<(), String> {
    // In the vein of "MVP": let's start by just loading all the files. We'll
    // extract this all into standalone functions as necessary later.

    // TODO: load config!
    //
    // Instead of just loading the files in the source directory as a glob of
    // all Markdown files, load the *config* and let *it* specify the source of
    // the files to convert.
    let dir_str = format!(
        "{}/**/*.md",
        site.source_directory.to_str().ok_or(String::from("bad directory"))?
    );

    let markdown_files = glob(&dir_str).map_err(|err| format!("{:?}", err))?;

    // TODO: Iterate with `rayon::par_iter::for_each()`.
    for path_result in markdown_files {
        // TODO: extract this into a nice function to call in a for loop/foreach.
        let path = path_result.map_err(|e| format!("{:?}", e))?;
        let file_name = path.to_str()
            .ok_or(format!("Could not convert path {:?} to str", path))?;

        let mut pandoc = Pandoc::new();
        pandoc.set_input_format(InputFormat::Markdown)
            .set_output_format(OutputFormat::Html5)
            .add_options(&[PandocOption::Smart, PandocOption::NoHighlight])
            .add_input(file_name)
            .set_output(OutputKind::Pipe);

        let pandoc_output = pandoc.execute()
            .map_err(|err| format!("pandoc failed on {}:\n{:?}", file_name, err))?;

        let converted = match pandoc_output {
            PandocOutput::ToFile(path_buf) => {
                let msg = format!(
                    "We wrote to a file ({}) instead of a pipe. That was weird.",
                    path_buf.to_string_lossy()
                );
                return Err(msg);
            },
            PandocOutput::ToBuffer(string) => string,
        };

        let highlighted = syntax_highlight(converted);

        // TODO: extract this as part of the writing it out process.
        // TODO: set output location in config.
        let ff_path = Path::new(file_name);
        let dest = Path::new("./tests/output")
            .join(ff_path.file_name().ok_or(format!("invalid file: {}", file_name))?)
            .with_extension("html");

        let mut fd = match File::create(dest.clone()) {
            Ok(file) => file,
            Err(reason) => {
                return Err(format!(
                    "Could not open {} for write: {}",
                    dest.to_string_lossy(), reason
                ));
            }
        };

        let result = write!(fd, "{}", highlighted);
        if let Err(reason) = result {
            return Err(format!("{:?}", reason.kind()));
        }
    }

    Ok(())
}

const CONTENT_DIRECTORIES: &'static str = "content_directories";
const TEMPLATE_DIRECTORY: &'static str = "template_directory";

struct Config {
    content_directories: Vec<PathBuf>,
    template_directory: PathBuf,
}

fn load_config(directory: &PathBuf) -> Result<Config, String> {
    const CONFIG_FILE: &'static str = "lightning.toml";
    let config_path = directory.join(CONFIG_FILE);
    if !config_path.exists() {
        return Err(format!(
            "The specified configuration path {:?} does not exist",
            config_path.to_string_lossy()
        ));
    }

    let mut file = File::open(&config_path)
        .map_err(|reason| format!("Error reading {:?}: {:?}", config_path, reason))?;

    let mut contents = String::new();
    file.read_to_string(&mut contents);

    let mut parser = toml::Parser::new(&contents);
    let parsed_table = match parser.parse() {
        Some(table) => table,
        None => {
            return Err(format!(
                "Could not parse the contents of {} as TOML. Errors include:\n{}",
                config_path.to_string_lossy(),
                parser.errors.into_iter()
                    .map(|err| format!("{}", err))
                    .collect::<Vec<String>>()
                    .join("\n")
            ));
        }
    };

    // TODO: extract conversion of parsed table to values into a testable function.
    use toml::Value;
    let content_directories = match parsed_table.get(CONTENT_DIRECTORIES) {
        Some(&Value::Array(ref values)) => {
            values.into_iter()
                .map(|value| {
                    match value {
                        &Value::String(ref string) => Some(PathBuf::from(string)),
                        _ => None,
                    }
                })
                .filter(|option| option.is_some())
                .map(|known_valid| known_valid.unwrap())
                .collect::<Vec<PathBuf>>()
        },
        Some(&Value::String(ref string)) => vec![PathBuf::from(string)],
        Some(_) => {
            return Err(format!(
                "Wrong value type at key {} in configuration file {}",
                TEMPLATE_DIRECTORY,
                config_path.to_string_lossy(),
            ));
        },
        None => {
            return Err(format!(
                "Could not load value from key {} in configuration file {}",
                CONTENT_DIRECTORIES,
                config_path.to_string_lossy(),
            ));
        },
    };

    let template_directory = match parsed_table.get(TEMPLATE_DIRECTORY) {
        Some(&Value::String(ref string)) => PathBuf::from(string),
        Some(_) => {
            return Err(format!(
                "Wrong value type at key {} in configuration file {}",
                TEMPLATE_DIRECTORY,
                config_path.to_string_lossy(),
            ));
        },
        None => {
            return Err(format!(
                "Could not load value from key {} in configuration file {}",
                TEMPLATE_DIRECTORY,
                config_path.to_string_lossy(),
            ));
        },
    };

    Ok(Config {
        content_directories: content_directories,
        template_directory: template_directory,
    })
}
