# Media Tag
Media tag is a simple tagging tool. You can use it to tag your locally downloaded media.

Output of help command:
```shell
Usage: mtag <COMMAND>

Commands:
  init        Initialize a media tag directory (create the database file)
  create-tag  Create a new tag
  show-tags   Print all tags
  search      Search tagged files
  status      Get a list of all tagged files along with their tags
  add         Tag one or more files with one or more tags
  remove      Remove one or more tags from one or more files
  help        Print this message or the help of the given subcommand(s)

Options:
  -h, --help     Print help
  -V, --version  Print version
```

Example usage:
```shell
mtag search chill --not piano | mpv --playlist=- --shuffle
```
