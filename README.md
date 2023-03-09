# Repeat

A simple clipboard manager for X11.

## Usage

Start the server with `rpt` and then show it with `rpt show`. You can also
pause it with `rpt pause`, and unpause with `rpt start`.

`rpt show` pops up the latest clips. Typing will start fuzzy searching through
the clips.

When the popup is showing:

- `Enter` will paste the chosen clip into the focused window.
- `Ctrl` + `Enter` will put the chosen clip into the clipboard but not paste it.
- `Up` or `Ctrl` + `K` will move up one clip.
- `Down` or `Ctrl` + `J` will move down one clip.
- `Ctrl` + `U` will erase the search.
- Any other character will be appended to the fuzzy search.

## Installation

Clone and install with `cargo install --path .`.
