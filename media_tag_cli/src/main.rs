use std::process::exit;
use std::{env, path::PathBuf};

use clap::{Parser, Subcommand};
use media_tag_lib::MediaTag;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Initialize a media tag directory (create the database file)
    Init,
    /// Create a new tag
    CreateTag { tags: Vec<String> },
    /// Print all tags
    ShowTags,
    /// Search tagged files
    Search {
        /// Look for files containing any of the provided tags
        #[arg(short, long)]
        any: bool,

        /// The tags you are looking for
        #[arg(num_args = 1..)]
        queries: Vec<String>,

        /// The tags you want to exclude
        #[arg(long = "not", num_args = 1..)]
        exclude: Vec<String>,
    },
    /// Get a list of all tagged files along with their tags
    Status,
    /// Tag one or more files with one or more tags
    Add { parameters: Vec<String> },
    /// Remove one or more tags from one or more files
    Remove { parameters: Vec<String> },
}

const DB_FILENAME: &str = ".media_tag.db";

fn parse_args(parameters: Vec<String>) -> (Vec<PathBuf>, Vec<String>) {
    let mut paths = Vec::new();
    let mut tags = Vec::new();

    for parameter in parameters {
        let path = PathBuf::from(&parameter);
        if path.exists() {
            paths.push(path);
        } else {
            tags.push(parameter);
        }
    }
    (paths, tags)
}

fn find_db_path() -> Option<PathBuf> {
    let current_dir = env::current_dir().ok()?;
    for dir in current_dir.ancestors() {
        let db_path = dir.join(DB_FILENAME);
        if db_path.exists() {
            return Some(db_path);
        }
    }
    None
}

fn main() {
    let args = Args::parse();

    if let Commands::Init = args.command {
        let path = PathBuf::from(DB_FILENAME);
        if path.exists() {
            eprintln!("'{DB_FILENAME}' already exists in this directory.");
            return;
        }
        if let Err(e) = MediaTag::new(DB_FILENAME) {
            print_error_and_exit(e);
        }
        println!("Initialized empty media-tag database in {}", path.display());
        return;
    }

    let db_path = match find_db_path() {
        Some(path) => path,
        None => {
            eprintln!(
                "fatal: not a media-tag repository (or any of the parent directories): {DB_FILENAME}"
            );
            exit(1);
        }
    };

    let media_tag = MediaTag::new(&db_path).unwrap_or_else(|err| print_error_and_exit(err));

    match args.command {
        Commands::Init => unreachable!(),
        Commands::CreateTag { tags } => {
            for tag in tags {
                media_tag.create_tag(&tag).unwrap_or_else(print_error);
            }
        }
        Commands::ShowTags => {
            let tags = media_tag
                .get_tags()
                .unwrap_or_else(|err| print_error_and_exit(err));

            for tag in tags {
                println!("{}", tag.name);
            }
        }
        Commands::Search {
            any,
            queries,
            exclude,
        } => {
            let media_tag_data = media_tag
                .load_media_tag()
                .unwrap_or_else(|err| print_error_and_exit(err));

            media_tag_data
                .media
                .iter()
                .filter(|medium| {
                    let has_tag = |query_tag: &String| {
                        medium.tags.iter().any(|&tag_id| {
                            media_tag_data
                                .tags
                                .get(&tag_id)
                                .map_or(false, |name| name == query_tag)
                        })
                    };

                    let matches_positive = if any {
                        if queries.is_empty() {
                            true
                        } else {
                            queries.iter().any(has_tag)
                        }
                    } else {
                        queries.iter().all(has_tag)
                    };

                    let matches_negative = exclude.iter().any(has_tag);

                    matches_positive && !matches_negative
                })
                .for_each(|medium| println!("{}", medium.path.display()));
        }
        Commands::Status => {
            let media_tag_data = media_tag
                .load_media_tag()
                .unwrap_or_else(|err| print_error_and_exit(err));

            for media in &media_tag_data.media {
                let tag_names: Vec<&str> = media
                    .tags
                    .iter()
                    .filter_map(|id| media_tag_data.tags.get(id).map(|s| s.as_str()))
                    .collect();

                println!("{} - {}", media.path.display(), tag_names.join(","));
            }
        }
        Commands::Add { parameters } => {
            let (paths, tags) = parse_args(parameters);
            for path in &paths {
                for tag in &tags {
                    media_tag.add_tag(path, tag).unwrap_or_else(|err| {
                        eprintln!("failed to add tag '{}' to '{}'", tag, path.display());
                        print_error(err);
                    });
                }
            }
        }
        Commands::Remove { parameters } => {
            let (paths, tags) = parse_args(parameters);
            for path in &paths {
                for tag in &tags {
                    media_tag.remove_tag(path, tag).unwrap_or_else(|err| {
                        eprintln!("failed to remove tag '{}' from '{}'", tag, path.display());
                        print_error(err);
                    });
                }
            }
        }
    }
}

fn print_error(e: impl std::error::Error) {
    eprintln!("error: {}", e);
    let mut source = e.source();
    while let Some(inner) = source {
        eprintln!("  caused by: {}", inner);
        source = inner.source();
    }
}

fn print_error_and_exit(e: impl std::error::Error) -> ! {
    print_error(e);
    exit(1);
}
