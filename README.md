# Repeat

A simple clipboard manager for X11.

## Usage

Start the server with `rpt` and then show it with `rpt show`. You can also
pause it with `rpt pause`, and unpause with `rpt start`.

`rpt show` pops up the latest clips. Typing will start fuzzy searching through
the clips. `Enter` will paste it in the window focused before the window
popped up. `Ctrl` + `Enter` will put the chosen clip into both `primary` and
`clipboard` selections for later use.

## Installation

Clone and install with `cargo install --path .`.
