//! Render the readme.md using the gpu renderer

use comrak::{markdown_to_html_with_plugins, ExtensionOptions, Options, Plugins, RenderOptions};

pub(crate) const GITHUB_MD_STYLES: &str = include_str!("../assets/github-markdown.css");
pub(crate) const BLITZ_MD_STYLES: &str = include_str!("../assets/blitz-markdown-overrides.css");

pub(crate) fn markdown_to_html(contents: String) -> String {
    let plugins = Plugins::default();
    // let syntax_highligher = CustomSyntectAdapter(SyntectAdapter::new(Some("InspiredGitHub")));
    // plugins.render.codefence_syntax_highlighter = Some(&syntax_highligher as _);

    let body_html = markdown_to_html_with_plugins(
        &contents,
        &Options {
            extension: ExtensionOptions {
                strikethrough: true,
                tagfilter: false,
                table: true,
                autolink: true,
                tasklist: true,
                superscript: false,
                header_ids: None,
                footnotes: false,
                description_lists: false,
                front_matter_delimiter: None,
                multiline_block_quotes: false,
                ..ExtensionOptions::default()
            },
            render: RenderOptions {
                unsafe_: true,
                tasklist_classes: true,
                ..RenderOptions::default()
            },
            ..Options::default()
        },
        &plugins,
    );

    format!(
        r#"
        <!DOCTYPE html>
        <html>
        <body>
        <div class="markdown-body">{}</div>
        </body>
        </html>
    "#,
        body_html
    )
}

// #[allow(unused)]
// mod syntax_highlighter {
//     use comrak::adapters::SyntaxHighlighterAdapter;
//     use comrak::plugins::syntect::SyntectAdapter;
//     use std::collections::HashMap;

//     struct CustomSyntectAdapter(SyntectAdapter);

//     impl SyntaxHighlighterAdapter for CustomSyntectAdapter {
//         fn write_highlighted(
//             &self,
//             output: &mut dyn std::io::Write,
//             lang: Option<&str>,
//             code: &str,
//         ) -> std::io::Result<()> {
//             let norm_lang = lang.map(|l| l.split_once(',').map(|(lang, _)| lang).unwrap_or(l));
//             self.0.write_highlighted(output, norm_lang, code)
//         }

//         fn write_pre_tag(
//             &self,
//             output: &mut dyn std::io::Write,
//             attributes: HashMap<String, String>,
//         ) -> std::io::Result<()> {
//             self.0.write_pre_tag(output, attributes)
//         }

//         fn write_code_tag(
//             &self,
//             output: &mut dyn std::io::Write,
//             attributes: HashMap<String, String>,
//         ) -> std::io::Result<()> {
//             self.0.write_code_tag(output, attributes)
//         }
//     }
// }
