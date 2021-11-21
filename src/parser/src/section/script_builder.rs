use std::{
    collections::HashMap,
    convert::TryInto,
    num::ParseIntError,
};

use kansei_core::types::{
    InvalidScriptPrefixError,
    Script,
    ScriptConfig,
};
use nix::sys::signal::Signal;
use snafu::{
    OptionExt,
    ResultExt,
    Snafu,
};

use super::SectionBuilder;

#[derive(Snafu, Debug)]
pub enum ScriptBuilderError {
    #[snafu(display("no prefix found"))]
    NoPrefixFound,
    #[snafu(display("{} must be either 'yes' or 'no'", key))]
    InvalidBoolean { key: String },
    #[snafu(display("failed conversion to integer for key {}", key))]
    InvalidInteger { key: String, source: ParseIntError },
    #[snafu(display("{}", source))]
    InvalidPrefix { source: InvalidScriptPrefixError },
    #[snafu(display("invalid signal"))]
    InvalidSignal { source: nix::Error },
    #[snafu(display("no execute found"))]
    NoExecuteFound,
}

pub struct ScriptBuilder {
    name: &'static str,
    pub script: Option<Result<Script, ScriptBuilderError>>,
}

type Result<T, E = ScriptBuilderError> = std::result::Result<T, E>;

impl ScriptBuilder {
    pub fn new_for_section(name: &'static str) -> Self {
        ScriptBuilder { name, script: None }
    }
}

fn get_int_or_default<T>(
    values: &mut HashMap<&'static str, String>,
    key: &'static str,
    default: T,
) -> Result<T, ScriptBuilderError>
where
    T: std::str::FromStr<Err = ParseIntError>,
{
    values
        .remove(key)
        .map_or(Ok(default), |value| value.parse())
        .with_context(|| {
            InvalidInteger {
                key: key.to_string(),
            }
        })
}

impl SectionBuilder for ScriptBuilder {
    fn build(
        &mut self,
        values: &mut HashMap<&'static str, String>,
        _array_values: &mut HashMap<&'static str, Vec<String>>,
        code_values: &mut HashMap<&'static str, String>,
    ) {
        let args: (&mut HashMap<&str, String>,) = (values,);
        self.script = Some(FnMut::call_mut(
            &mut move |values: &mut HashMap<&'static str, String>| -> Result<Script, ScriptBuilderError> {
                let prefix = values
                    .remove("prefix")
                    .with_context(|| NoPrefixFound)?
                    .try_into()
                    .with_context(|| InvalidPrefix)?;
                let execute = code_values
                    .remove("execute")
                    .with_context(|| NoExecuteFound)?;
                let config = ScriptConfig::new();
                let timeout =
                    get_int_or_default(values, "timeout", Script::DEFAULT_SCRIPT_TIMEOUT)?;
                let timeout_kill = get_int_or_default(
                    values,
                    "timeout_kill",
                    Script::DEFAULT_SCRIPT_TIMEOUT_KILL,
                )?;
                let max_deaths = get_int_or_default(
                    values,
                    "max_deaths",
                    Script::DEFAULT_SCRIPT_MAX_DEATHS,
                )?;
                let down_signal = values
                    .remove("down_signal")
                    .map_or(Ok(Signal::SIGINT), |down_signal| down_signal.parse())
                    .with_context(|| InvalidSignal)? as i32;

                let autostart = values
                    .remove("autostart")
                    .map_or(Ok(true), |autostart| {
                        match autostart.as_str() {
                            "yes" => Ok(true),
                            "no" => Ok(false),
                            _ => Err(snafu::NoneError),
                        }
                    })
                    .with_context(|| {
                        InvalidBoolean {
                            key: "autostart".to_string(),
                        }
                    })?;
                let user = values.remove("user");
                let group = values.remove("group");
                let notify = values
                    .remove("notify")
                    .map_or(Ok(None), |notify| {
                        notify.parse::<u8>().map(Some)
                    })
                    .with_context(|| {
                        InvalidInteger {
                            key: "notify".to_string(),
                        }
                    })?;
                Ok(Script {
                    prefix,
                    execute,
                    config,
                    timeout,
                    timeout_kill,
                    max_deaths,
                    down_signal,
                    autostart,
                    user,
                    group,
                    notify,
                })
            },
            args,
        ));
    }

    fn section_name(&self) -> &'static str {
        self.name
    }

    fn get_fields(&self) -> &'static [&'static str] {
        &[
            "prefix",
            "timeout",
            "timeout_kill",
            "max_deaths",
            "down_signal",
            "autostart",
            "user",
            "group",
            "notify",
        ]
    }

    fn get_array_fields(&self) -> &'static [&'static str] {
        &[]
    }

    fn get_code_fields(&self) -> &'static [&'static str] {
        &["execute"]
    }
}

#[cfg(test)]
mod test {
    use kansei_core::types::ScriptPrefix;

    use super::*;

    #[test]
    fn parse_script() {
        let mut builder = ScriptBuilder::new_for_section("start");
        assert!(
            builder
                .parse_until_next_section(&[
                    "prefix = bash".to_string(),
                    "execute = (".to_string(),
                    "    exit 0".to_string(),
                    ")".to_string(),
                ])
                .unwrap()
                .is_empty()
        );

        let script = builder.script.unwrap().unwrap();
        assert_eq!(script.prefix, ScriptPrefix::Bash);
        assert_eq!(script.execute, "    exit 0".to_string());
    }
}
