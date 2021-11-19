use std::ffi::CString;

use anyhow::{
    Context,
    Result,
};
use kansei_core::types::{
    ScriptConfig,
    ScriptPrefix,
};

pub struct ExecArgs {
    pub exe: CString,
    pub args: Vec<CString>,
    pub env: Vec<CString>,
}

impl ExecArgs {
    pub fn new(
        prefix: &ScriptPrefix,
        execute: &str,
        env: ScriptConfig,
    ) -> Result<Self> {
        let exe = CString::new(match prefix {
            ScriptPrefix::Bash => "bash",
            ScriptPrefix::Path => {
                execute
                    .split_whitespace()
                    .next()
                    .filter(|word| word.chars().all(char::is_alphabetic))
                    .unwrap_or("")
            }
            ScriptPrefix::Sh => "sh",
        })
        .context("unable to create C string")?;

        let args = match prefix {
            ScriptPrefix::Bash => {
                vec![CString::new("bash").context("unable to create C string")?]
            }
            ScriptPrefix::Path => todo!(),
            ScriptPrefix::Sh => {
                vec![CString::new("sh").context("unable to create C string")?]
            }
        };

        Ok(Self {
            exe,
            args,
            env: unimplemented!(),
        })
    }
}
