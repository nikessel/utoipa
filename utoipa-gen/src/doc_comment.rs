use std::path::PathBuf;
use syn::{Attribute, Expr, Lit, Meta, MetaNameValue};

const DOC_ATTRIBUTE_TYPE: &str = "doc";

/// CommentAttributes holds Vec of parsed doc comments
#[cfg_attr(feature = "debug", derive(Debug))]
pub(crate) struct CommentAttributes(pub(crate) Vec<String>);

impl CommentAttributes {
    /// Creates new [`CommentAttributes`] instance from [`Attribute`] slice filtering out all
    /// other attributes which are not `doc` comments
    pub(crate) fn from_attributes(attributes: &[Attribute]) -> Self {
        let mut docs = attributes
            .iter()
            .filter_map(|attr| {
                if !matches!(attr.path().get_ident(), Some(ident) if ident == DOC_ATTRIBUTE_TYPE) {
                    return None;
                }

                // ignore `#[doc(hidden)]` and similar tags.
                if let Meta::NameValue(name_value) = &attr.meta {
                    return Self::extract_doc_value(name_value);
                }
                None
            })
            .collect::<Vec<_>>();

        // Calculate the minimum indentation of all non-empty lines and strip them.
        let min_indent = docs
            .iter()
            .filter(|s| !s.is_empty())
            .map(|s| s.len() - s.trim_start_matches(' ').len())
            .min()
            .unwrap_or(0);

        for line in &mut docs {
            if !line.is_empty() {
                line.drain(..min_indent);
            }
        }

        Self(docs)
    }

    /// Extract documentation value from a name-value pair, handling both string literals
    /// and include_str! macro expressions
    fn extract_doc_value(name_value: &MetaNameValue) -> Option<String> {
        match &name_value.value {
            // Handle direct string literals
            Expr::Lit(doc_comment) => {
                if let Lit::Str(doc) = &doc_comment.lit {
                    let mut doc = doc.value();
                    doc.truncate(doc.trim_end().len());
                    Some(doc)
                } else {
                    None
                }
            }
            // Handle macro calls (like include_str!)
            Expr::Macro(macro_expr) => {
                if macro_expr.mac.path.is_ident("include_str") {
                    Some(Self::evaluate_include_str(
                        &macro_expr.mac.tokens.to_string(),
                    ))
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    /// Evaluates include_str! macro at compile time
    fn evaluate_include_str(path_str: &str) -> String {
        // Clean up the path string - remove quotes and whitespace
        let path_str = path_str.trim().trim_matches('"');

        // Check if the path contains CARGO_MANIFEST_DIR
        if path_str.contains("CARGO_MANIFEST_DIR") {
            return Self::evaluate_manifest_dir_path(path_str);
        }

        // Handle direct paths
        std::fs::read_to_string(path_str).unwrap_or_else(|err| {
            panic!("Failed to read include_str! file: {}", err);
        })
    }

    /// Evaluates paths that use CARGO_MANIFEST_DIR
    fn evaluate_manifest_dir_path(path_str: &str) -> String {
        // Get CARGO_MANIFEST_DIR from environment
        let manifest_dir = std::env::var("CARGO_MANIFEST_DIR")
            .unwrap_or_else(|_| panic!("CARGO_MANIFEST_DIR not found in environment"));

        // Extract the path after CARGO_MANIFEST_DIR
        let processed_path = path_str
            .replace("concat!", "")
            .replace("env!(\"CARGO_MANIFEST_DIR\")", "")
            .replace("env!('CARGO_MANIFEST_DIR')", "")
            .replace(',', "");

        let relative_path = processed_path
            .trim()
            .trim_matches(|c| c == '(' || c == ')')
            .trim()
            .trim_matches('"');

        // Combine paths
        let full_path = PathBuf::from(manifest_dir).join(relative_path.trim_start_matches('/'));

        let path_str = full_path
            .to_str()
            .unwrap_or_else(|| panic!("Invalid path: {:?}", full_path));

        std::fs::read_to_string(path_str)
            .unwrap_or_else(|err| panic!("Failed to read include_str! file: {}", err))
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Returns found `doc comments` as formatted `String` joining them all with `\n` *(new line)*.
    pub(crate) fn as_formatted_string(&self) -> String {
        self.0.join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;
    use syn::parse_quote;
    use tempdir::TempDir;

    #[test]
    fn test_basic_doc_comment() {
        let attr: Attribute = parse_quote!(#[doc = "Basic doc comment"]);
        let comments = CommentAttributes::from_attributes(&[attr]);
        assert_eq!(comments.as_formatted_string(), "Basic doc comment");
    }

    #[test]
    fn test_manifest_dir_path() {
        // Create a temporary directory
        let tmp_dir = TempDir::new("doc_test").unwrap();
        let test_file_path = tmp_dir.path().join("test_doc.txt");
        let test_content = "Test content";

        // Create a temporary file in the temporary directory
        {
            let mut file = File::create(&test_file_path).unwrap();
            write!(file, "{}", test_content).unwrap();
        }

        // Set CARGO_MANIFEST_DIR to our temporary directory
        std::env::set_var("CARGO_MANIFEST_DIR", tmp_dir.path());

        // Use just the filename in the include_str! macro
        let path_str = format!(
            "concat!(env!(\"CARGO_MANIFEST_DIR\"), \"/{}\")",
            test_file_path.file_name().unwrap().to_str().unwrap()
        );

        let result = CommentAttributes::evaluate_include_str(&path_str);

        // TempDir will automatically clean up the directory and its contents when it goes out of scope
        assert_eq!(result, test_content);
    }
}
