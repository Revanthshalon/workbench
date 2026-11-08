#![allow(clippy::todo)]

use std::error::Error;
use std::fmt::Display;

#[derive(Debug)]
pub enum McpErrors {}

impl Display for McpErrors {
    fn fmt(&self, _f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        todo!()
    }
}

impl Error for McpErrors {}
