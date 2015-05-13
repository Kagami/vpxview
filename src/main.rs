#![feature(libc)]
#![feature(exit_status)]

extern crate libc;
#[macro_use]
extern crate gfx;
extern crate gfx_device_gl;
extern crate gfx_window_glutin;
extern crate glutin;
extern crate gfx_text;

use std::env;
#[macro_use]
mod common;
mod ivf;
mod gui;
mod vpx;

fn run(filename: String) -> Result<(), common::Error> {
    let decoder = try!(vpx::Decoder::init());
    let reader = try!(ivf::Reader::open(filename));
    try!(gui::init(reader, decoder)).run();
    Ok(())
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        printerr!("Usage: {} file.ivf", args[0]);
        return env::set_exit_status(1);
    }
    match run(args[1].clone()) {
        Err(err) => {
            printerr!("Cannot proceed due to {}", err);
            return env::set_exit_status(1);
        },
        // Success exit.
        Ok(_) => return,
    }
}
