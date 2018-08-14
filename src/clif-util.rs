#![deny(trivial_numeric_casts, unstable_features, unused_extern_crates)]
#![warn(unused_import_braces)]
#![cfg_attr(
    feature = "cargo-clippy",
    warn(
        float_arithmetic, mut_mut, nonminimal_bool, option_map_unwrap_or, option_map_unwrap_or_else,
        unicode_not_nfc, use_self
    )
)]

extern crate file_per_thread_logger;
#[macro_use]
extern crate cfg_if;
#[cfg(feature = "disas")]
extern crate capstone;
extern crate clap;
extern crate cranelift_codegen;
extern crate cranelift_entity;
extern crate cranelift_filetests;
extern crate cranelift_reader;
extern crate filecheck;
extern crate pretty_env_logger;
extern crate term;

cfg_if! {
    if #[cfg(feature = "wasm")] {
        extern crate cranelift_wasm;
        extern crate wabt;
        mod wasm;
    }
}
extern crate target_lexicon;

use clap::{App, Arg, SubCommand};
use cranelift_codegen::dbg::LOG_FILENAME_PREFIX;
use cranelift_codegen::VERSION;
use std::io::{self, Write};
use std::option::Option;
use std::process;

mod cat;
mod compile;
mod print_cfg;
mod rsfilecheck;
mod utils;

/// A command either succeeds or fails with an error message.
pub type CommandResult = Result<(), String>;

fn add_input_file_arg<'a>() -> clap::Arg<'a, 'a> {
    Arg::with_name("file")
        .required(true)
        .multiple(true)
        .value_name("file")
        .help("Specify file(s) to be used for test")
}

fn add_verbose_flag<'a>() -> clap::Arg<'a, 'a> {
    Arg::with_name("verbose").short("v").help("Be more verbose")
}

fn add_time_flag<'a>() -> clap::Arg<'a, 'a> {
    Arg::with_name("time-passes")
        .short("T")
        .help("Print pass timing report for test")
}

fn add_set_flag<'a>() -> clap::Arg<'a, 'a> {
    Arg::with_name("set")
        .long("set")
        .takes_value(true)
        .multiple(true)
        .help("Configure Cranelift settings")
}

fn add_target_flag<'a>() -> clap::Arg<'a, 'a> {
    Arg::with_name("target")
        .takes_value(true)
        .long("target")
        .help("Specify the Cranelift target")
}

fn add_print_flag<'a>() -> clap::Arg<'a, 'a> {
    Arg::with_name("print")
        .short("p")
        .help("Print the resulting Cranelift IR")
}

fn add_debug_flag<'a>() -> clap::Arg<'a, 'a> {
    Arg::with_name("debug")
        .short("d")
        .help("enable debug output on stderr/stdout")
}

/// Takes vector of clap values and converts the values to strings and puts values in the mut vector
fn get_vec<'a>(mut_vec: &mut Vec<String>, argument_vec: Option<clap::Values<'a>>) {
    if let Some(clap_vec) = argument_vec {
        for val in clap_vec {
            mut_vec.push(val.to_string());
        }
    }
}

fn add_wasm_or_compile<'a>(cmd: &str) -> clap::App<'a, 'a> {
    let about_str = match cmd {
        "wasm" => "Compiles Cranelift IR into target language",
        "compile" => "Compiles Cranelift IR into target language",
        _ => panic!("Invalid command"),
    };

    SubCommand::with_name(cmd)
        .about(about_str)
        .arg(add_verbose_flag())
        .arg(add_print_flag())
        .arg(add_time_flag())
        .arg(add_set_flag())
        .arg(add_target_flag())
        .arg(add_input_file_arg())
        .arg(add_debug_flag())
}

fn handle_debug_flag(debug: bool) {
    if debug {
        pretty_env_logger::init();
    } else {
        file_per_thread_logger::initialize(LOG_FILENAME_PREFIX);
    }
}

fn main() {
    let app_cmds = App::new("Cranelift code generator utility")
        .version(VERSION)
        .subcommand(
            SubCommand::with_name("test")
                .about("Run Cranelift tests")
                .arg(add_verbose_flag())
                .arg(add_time_flag())
                .arg(add_input_file_arg())
                .arg(add_debug_flag()),
        )
        .subcommand(
            SubCommand::with_name("cat")
                .about("Outputs .clif file")
                .arg(add_input_file_arg())
                .arg(add_debug_flag()),
        )
        .subcommand(
            SubCommand::with_name("print-cfg")
                .about("Prints out cfg in dot format")
                .arg(add_input_file_arg())
                .arg(add_debug_flag()),
        )
        .subcommand(
            add_wasm_or_compile("compile")
                .arg(
                    Arg::with_name("just-decode")
                        .short("t")
                        .help("Just decode WebAssembly to Cranelift IR"),
                )
                .arg(Arg::with_name("check-translation").short("c").help(
                    "Just checks the correctness of Cranelift IR translated from WebAssembly",
                )),
        )
        .subcommand(add_wasm_or_compile("wasm"));

    let res_util = match app_cmds.get_matches().subcommand() {
        ("cat", Some(rest_cmd)) => {
            handle_debug_flag(rest_cmd.is_present("debug"));

            let mut file_vec: Vec<String> = Vec::new();
            get_vec(&mut file_vec, rest_cmd.values_of("file"));
            cat::run(&file_vec)
        }
        ("test", Some(rest_cmd)) => {
            handle_debug_flag(rest_cmd.is_present("debug"));

            let mut file_vec: Vec<String> = Vec::new();
            get_vec(&mut file_vec, rest_cmd.values_of("file"));
            cranelift_filetests::run(rest_cmd.is_present("time-passes"), &file_vec).map(|_time| ())
        }
        ("filecheck", Some(rest_cmd)) => {
            handle_debug_flag(rest_cmd.is_present("debug"));

            let mut file_vec: Vec<String> = Vec::new();
            get_vec(&mut file_vec, rest_cmd.values_of("file"));
            rsfilecheck::run(&file_vec, rest_cmd.is_present("verbose"))
        }
        ("print-cfg", Some(rest_cmd)) => {
            handle_debug_flag(rest_cmd.is_present("debug"));

            let mut file_vec: Vec<String> = Vec::new();
            get_vec(&mut file_vec, rest_cmd.values_of("file"));
            print_cfg::run(&file_vec)
        }
        ("compile", Some(rest_cmd)) => {
            handle_debug_flag(rest_cmd.is_present("debug"));

            let mut file_vec: Vec<String> = Vec::new();
            get_vec(&mut file_vec, rest_cmd.values_of("file"));

            let mut set_vec: Vec<String> = Vec::new();
            get_vec(&mut set_vec, rest_cmd.values_of("set"));

            let mut target_val: &str = "";
            if let Some(clap_target_vec) = rest_cmd.value_of("target") {
                target_val = clap_target_vec;
            }
            compile::run(file_vec, rest_cmd.is_present("print"), &set_vec, target_val)
        }
        ("wasm", Some(rest_cmd)) => {
            handle_debug_flag(rest_cmd.is_present("debug"));

            let mut file_vec: Vec<String> = Vec::new();
            get_vec(&mut file_vec, rest_cmd.values_of("file"));

            let mut set_vec: Vec<String> = Vec::new();
            get_vec(&mut set_vec, rest_cmd.values_of("set"));

            let mut target_val: &str = "";
            if let Some(clap_target_vec) = rest_cmd.values_of("target") {
                for val in clap_target_vec {
                    target_val = val;
                }
            }

            #[cfg(feature = "wasm")]
            let result = wasm::run(
                file_vec,
                rest_cmd.is_present("verbose"),
                rest_cmd.is_present("just-decode"),
                rest_cmd.is_present("check-translation"),
                rest_cmd.is_present("print"),
                &set_vec,
                target_val,
                rest_cmd.is_present("print-size"),
            );

            #[cfg(not(feature = "wasm"))]
            let result = Err("Error: clif-util was compiled without wasm support.".to_owned());

            result
        }
        _ => Err(format!("Invalid subcommand.")),
    };

    if let Err(mut msg) = res_util {
        if !msg.ends_with('\n') {
            msg.push('\n');
        }
        io::stdout().flush().expect("flushing stdout");
        io::stderr().write_all(msg.as_bytes()).unwrap();
        process::exit(1);
    }
}
