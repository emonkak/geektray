#![allow(non_camel_case_types, non_snake_case, non_upper_case_globals, unused)]
#![cfg_attr(test, allow(deref_nullptr))]

use nix::libc::FILE;

include!(concat!(env!("OUT_DIR"), "/xkbcommon_sys.rs"));
