use kansei_core::types::{
    Bundle,
    Service,
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
        SectionBuilder,
        SectionBuilderError,
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
    pub fn parse(&mut $self, lines: &[String]) -> Result<(), ServiceBuilderError> {
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

    parse_sections!(self, "options", self.options_builder);
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
}
