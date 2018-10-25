use failure::Error;
use rnix::parser::{ASTNode, Data};
use rnix::tokenizer::Meta;
use rnix::tokenizer::Trivia;
use rnix;
use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;
use structopt::StructOpt;
use xml::writer::{EventWriter, EmitterConfig, XmlEvent};

type Result<T> = std::result::Result<T, Error>;

/// Command line arguments for nixdoc
#[derive(Debug, StructOpt)]
#[structopt(name = "nixdoc", about = "Generate Docbook from Nix library functions")]
struct Options {
    /// Nix file to process.
    #[structopt(short = "f", long = "file", parse(from_os_str))]
    file: PathBuf,

    /// Name of the function category (e.g. 'strings', 'attrsets').
    #[structopt(short = "c", long = "category")]
    category: String,

    /// Description of the function category.
    #[structopt(short = "d", long = "description")]
    description: String,
}

#[derive(Debug)]
struct DocComment {
    /// Primary documentation string.
    doc: String,

    /// Optional type annotation for the thing being documented.
    doc_type: Option<String>,

    /// Usage example(s) (interpreted as a single code block)
    example: Option<String>,
}

#[derive(Debug)]
struct DocItem {
    name: String,
    comment: DocComment,
}

/// Represents a single function parameter and (potentially) its
/// documentation.
#[derive(Debug)]
struct Parameter {
    name: String,
    description: Option<String>,
    arg_type: Option<String>,
}

/// Represents a single manual section describing a library function.
#[derive(Debug)]
struct ManualEntry {
    /// Name of the ... (TODO: Word for this? attrsets, strings, trivial etc)
    category: String,

    /// Name of the section (used as the title)
    name: String,

    /// Type signature (if provided). This is not actually a checked
    /// type signature in any way.
    fn_type: Option<String>,

    /// Primary description of the entry.
    description: String, // TODO

    /// Parameters of the function
    parameters: Vec<Parameter>,
}

impl ManualEntry {
    /// Write a single DocBook entry for a documented Nix function.
    fn write_section_xml<W: Write>(&self, w: &mut EventWriter<W>) -> Result<()> {
        let ident = format!("lib.{}.{}", self.category, self.name);

        // <section ...
        w.write(XmlEvent::start_element("section")
                .attr("xml:id", format!("function-library-{}", ident).as_str()))?;

        // <title> ...
        w.write(XmlEvent::start_element("title"))?;
        w.write(XmlEvent::start_element("function"))?;
        w.write(XmlEvent::characters(ident.as_str()))?;
        w.write(XmlEvent::end_element())?;
        w.write(XmlEvent::end_element())?;

        // <subtitle> (type signature)
        if let Some(t) = &self.fn_type {
            w.write(XmlEvent::start_element("subtitle"))?;
            w.write(XmlEvent::start_element("literal"))?;
            w.write(XmlEvent::characters(t))?;
            w.write(XmlEvent::end_element())?;
            w.write(XmlEvent::end_element())?;
        }

        // Primary doc string
        // TODO: Split paragraphs?
        w.write(XmlEvent::start_element("para"))?;
        w.write(XmlEvent::characters(&self.description))?;
        w.write(XmlEvent::end_element())?;

        // </section>
        w.write(XmlEvent::end_element())?;

        Ok(())
    }
}

/// Retrieve documentation comments. For now only multiline comments
/// starting with `@doc` are considered.
fn retrieve_doc_comment(meta: &Meta) -> Option<String> {
    for item in meta.leading.iter() {
        if let Trivia::Comment { multiline, content, .. } = item {
            if *multiline { //  && content.as_str().starts_with(" @doc") {
                return Some(content.to_string())
            }
        }
    }

    return None;
}

/// Transforms an AST node into a `DocItem` if it has a leading
/// documentation comment.
fn retrieve_doc_item(node: &ASTNode) -> Option<DocItem> {
    // We are only interested in identifiers.
    if let Data::Ident(meta, name) = &node.data {
        let comment = retrieve_doc_comment(meta)?;

        return Some(DocItem {
            name: name.to_string(),
            comment: parse_doc_comment(&comment),
        })
    }

    return None;
}

/// *Really* dumb, mutable, hacky doc comment "parser".
fn parse_doc_comment(raw: &str) -> DocComment {
    enum ParseState { Doc, Type, Example }

    let mut doc = String::new();
    let mut doc_type = String::new();
    let mut example = String::new();
    let mut state = ParseState::Doc;

    for line in raw.trim().lines() {
        let mut line = line.trim();

        if line.starts_with("@doc ") {
            state = ParseState::Doc;
            line = line.trim_start_matches("@doc ");
        }

        if line.starts_with("Type:") {
            state = ParseState::Type;
            line = &line[5..]; //.trim_start_matches("Type:");
        }

        if line.starts_with("Example:") {
            state = ParseState::Example;
            line = line.trim_start_matches("Example:");
        }

        match state {
            ParseState::Type => doc_type.push_str(line.trim()),
            ParseState::Doc => {
                doc.push_str(line.trim());
                doc.push('\n');
            },
            ParseState::Example => {
                example.push_str(line.trim());
                example.push('\n');
            },
        }
    }


    let f = |s: String| if s.is_empty() { None } else { Some(s.into()) };

    DocComment {
        doc: doc.trim().into(),
        doc_type: f(doc_type),
        example: f(example),
    }
}

fn main() {
    let opts = Options::from_args();
    let src = fs::read_to_string(&opts.file).unwrap();
    let nix = rnix::parse(&src).unwrap();

    let entries: Vec<ManualEntry> = nix.arena.into_iter()
        .filter_map(retrieve_doc_item)
        .map(|d| ManualEntry {
            category: opts.category.clone(),
            name: d.name,
            description: d.comment.doc,
            fn_type: d.comment.doc_type,
            parameters: vec![],
        })
        .collect();

    let mut writer = EmitterConfig::new()
        .perform_indent(true)
        .create_writer(io::stdout());

    writer.write(
        XmlEvent::start_element("section")
            .attr("xmlns", "http://docbook.org/ns/docbook")
            .attr("xmlns:xlink", "http://www.w3.org/1999/xlink")
            .attr("xmlns:xi", "http://www.w3.org/2001/XInclude")
            .attr("xml:id", format!("sec-functions-library-{}", opts.category).as_str()))
        .unwrap();

    writer.write(XmlEvent::start_element("title")).unwrap();
    writer.write(XmlEvent::characters(&opts.description)).unwrap();
    writer.write(XmlEvent::end_element()).unwrap();

    for entry in entries {
        entry.write_section_xml(&mut writer).expect("Failed to write section")
    }

    writer.write(XmlEvent::end_element()).unwrap();
}
