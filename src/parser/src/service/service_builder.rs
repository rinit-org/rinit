use kansei_core::types::{
    Bundle,
    Longrun,
    Oneshot,
    Service,
    ServiceOptions,
};
use snafu::{
    ensure,
    Error,
    OptionExt,
    ResultExt,
    Snafu,
};

use crate::{
    is_empty_line::is_empty_line,
    parse_section::parse_section,
    section::{
        BundleOptionsBuilder,
        ScriptBuilder,
        SectionBuilder,
        SectionBuilderError,
        ServiceOptionsBuilder,
    },
};

#[derive(Snafu, Debug)]
pub enum ServiceBuilderError {
    #[snafu(display("cannot find any section"))]
    EmptyService,
    #[snafu(display("error in section {}: {}", section, source))]
    ErrorInSection {
        section: String,
        source: SectionBuilderError,
    },
    #[snafu(display("section header not found"))]
    SectionHeaderNotFound,
    #[snafu(display("{} is not a valid section", section))]
    InvalidSection { section: String },
}

macro_rules! parse_sections{
    ($self:ident, $( $section:expr, $builder:expr ),*) => {
    fn parse(&mut $self, lines: &[String]) -> Result<(), ServiceBuilderError> {
        use self::InvalidSection;
        let mut lines: &[String] = lines;
        let mut start = 0;
        let len = lines.len();
        loop {
            if len <= start || !is_empty_line(lines[start].trim()) {
                break
            }
            start += 1;
        }
        ensure!(len > start, EmptyService);
        lines = &lines[start..];
        loop {
            let section = parse_section(&lines[0]).with_context(|| SectionHeaderNotFound)?;
            lines = match section {
                $(
                    $section => {
                        $builder.parse_until_next_section(&lines[1..]).with_context(|| ErrorInSection { section: section.to_string() })?
                    },
                )*

                _ => { return InvalidSection { section: section.to_string() }.fail(); }
            };

            if lines.is_empty() {
                break;
            }
        }
        Ok(())
    }
}}

pub trait ServiceBuilder {
    fn build(self) -> Result<Service, Box<dyn snafu::Error>>;
    fn parse(
        &mut self,
        lines: &[String],
    ) -> Result<(), ServiceBuilderError>;
}

pub struct BundleBuilder {
    name: String,
    options_builder: BundleOptionsBuilder,
}

#[derive(Snafu, Debug)]
pub enum BundleBuilderError {
    #[snafu(display("no options section found"))]
    NoOptionsSection,
}

impl BundleBuilder {
    pub fn new(name: String) -> Self {
        Self {
            name,
            options_builder: BundleOptionsBuilder::new(),
        }
    }
}

pub struct OneshotBuilder {
    name: String,
    start_builder: ScriptBuilder,
    stop_builder: ScriptBuilder,
    options_builder: ServiceOptionsBuilder,
}

#[derive(Snafu, Debug)]
pub enum OneshotBuilderError {
    #[snafu(display("no start section found"))]
    NoStartSection,
}

impl OneshotBuilder {
    pub fn new(name: String) -> Self {
        Self {
            name,
            start_builder: ScriptBuilder::new_for_section("start"),
            stop_builder: ScriptBuilder::new_for_section("stop"),
            options_builder: ServiceOptionsBuilder::new(),
        }
    }
}

pub struct LongrunBuilder {
    name: String,
    run_builder: ScriptBuilder,
    finish_builder: ScriptBuilder,
    options_builder: ServiceOptionsBuilder,
}

#[derive(Snafu, Debug)]
pub enum LongrunBuilderError {
    #[snafu(display("no start section found"))]
    NoRunSection,
}

impl LongrunBuilder {
    pub fn new(name: String) -> Self {
        Self {
            name,
            run_builder: ScriptBuilder::new_for_section("run"),
            finish_builder: ScriptBuilder::new_for_section("finish"),
            options_builder: ServiceOptionsBuilder::new(),
        }
    }
}

impl ServiceBuilder for BundleBuilder {
    fn build(self) -> Result<Service, Box<dyn Error>> {
        Ok(Service::Bundle(Bundle {
            name: self.name,
            options: self
                .options_builder
                .bundle_options
                .with_context(|| NoOptionsSection)??,
        }))
    }

    parse_sections!(self, "options", self.options_builder);
}

impl ServiceBuilder for OneshotBuilder {
    fn build(self) -> Result<Service, Box<dyn Error>> {
        Ok(Service::Oneshot(Oneshot {
            name: self.name,
            start: self
                .start_builder
                .script
                .with_context(|| NoStartSection)??,
            stop: if let Some(stop) = self.stop_builder.script {
                Some(stop?)
            } else {
                None
            },
            options: if let Some(options) = self.options_builder.options {
                options?
            } else {
                ServiceOptions::new()
            },
        }))
    }

    parse_sections!(
        self,
        "start",
        self.start_builder,
        "stop",
        self.stop_builder,
        "options",
        self.options_builder
    );
}

impl ServiceBuilder for LongrunBuilder {
    fn build(self) -> Result<Service, Box<dyn Error>> {
        Ok(Service::Longrun(Longrun {
            name: self.name,
            run: self.run_builder.script.with_context(|| NoRunSection)??,
            finish: if let Some(finish) = self.finish_builder.script {
                Some(finish?)
            } else {
                None
            },
            options: if let Some(options) = self.options_builder.options {
                options?
            } else {
                ServiceOptions::new()
            },
        }))
    }

    parse_sections!(
        self,
        "run",
        self.run_builder,
        "finish",
        self.finish_builder,
        "options",
        self.options_builder
    );
}
