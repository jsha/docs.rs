use crate::web::page::TemplateData;
use lol_html::errors::RewritingError;
use tera::Context;

/// Rewrite a rustdoc page to have the docs.rs header
///
/// Given a rustdoc HTML page and a context to serialize it with,
/// render the `rustdoc/` templates with the `html`.
/// The output is an HTML page which has not yet been UTF-8 validated.
/// In practice, the output should always be valid UTF-8.
pub(crate) fn rewrite_lol(
    html: &[u8],
    max_allowed_memory_usage: usize,
    ctx: Context,
    templates: &TemplateData,
) -> Result<Vec<u8>, RewritingError> {
    use lol_html::html_content::{ContentType, Element};
    use lol_html::{ElementContentHandlers, HtmlRewriter, MemorySettings, Settings};

    let templates = templates.templates.load();
    let tera_head = templates.render("rustdoc/head.html", &ctx).unwrap();
    let tera_vendored_css = templates.render("rustdoc/vendored.html", &ctx).unwrap();
    let tera_body = templates.render("rustdoc/body.html", &ctx).unwrap();
    let tera_rustdoc_header = templates.render("rustdoc/header.html", &ctx).unwrap();

    // Append `style.css` stylesheet after all head elements.
    let head_handler = |head: &mut Element| {
        head.append(&tera_head, ContentType::Html);

        Ok(())
    };

    // Before: <body> ... rustdoc content ... </body>
    // After:
    // ```html
    // <div id="rustdoc_body_wrapper" class="{{ rustdoc_body_class }}" tabindex="-1">
    //      ... rustdoc content ...
    // </div>
    // ```
    let body_handler = |rustdoc_body_class: &mut Element| {
        // Add the `rustdoc` classes to the html body
        let mut tmp;
        let klass = if let Some(classes) = rustdoc_body_class.get_attribute("class") {
            tmp = classes;
            tmp.push_str(" container-rustdoc");
            &tmp
        } else {
            "container-rustdoc"
        };
        rustdoc_body_class.set_attribute("class", klass)?;
        rustdoc_body_class.set_attribute("id", "rustdoc_body_wrapper")?;
        rustdoc_body_class.set_attribute("tabindex", "-1")?;
        // Change the `body` to a `div`
        rustdoc_body_class.set_tag_name("div")?;
        // Prepend the tera content
        rustdoc_body_class.prepend(&tera_body, ContentType::Html);
        // Wrap the tranformed body and rustdoc header into a <body> element
        rustdoc_body_class.before("<body>", ContentType::Html);
        // Insert the header outside of the rustdoc div
        rustdoc_body_class.before(&tera_rustdoc_header, ContentType::Html);
        // Finalize body with </body>
        rustdoc_body_class.after("</body>", ContentType::Html);

        Ok(())
    };

    // Append `vendored.css` before `rustdoc.css`, so that the duplicate copy of
    // `normalize.css` will be overridden by the later version.
    let first_stylesheet_handler = |head: &mut Element| {
        head.before(&tera_vendored_css, ContentType::Html);

        Ok(())
    };

    let (head_selector, body_selector, first_stylesheet_selector) = (
        "head".parse().unwrap(),
        "body".parse().unwrap(),
        "link[type='text/css'][href*='rustdoc']".parse().unwrap(),
    );
    let element_content_handlers = vec![
        (
            &head_selector,
            ElementContentHandlers::default().element(head_handler),
        ),
        (
            &body_selector,
            ElementContentHandlers::default().element(body_handler),
        ),
        (
            &first_stylesheet_selector,
            ElementContentHandlers::default().element(first_stylesheet_handler),
        ),
    ];
    let settings = Settings {
        element_content_handlers,
        memory_settings: MemorySettings {
            max_allowed_memory_usage,
            ..MemorySettings::default()
        },
        ..Settings::default()
    };

    // The input and output are always strings, we just use `&[u8]` so we only have to validate once.
    let mut buffer = Vec::new();
    // TODO: Make the rewriter persistent?
    let mut writer = HtmlRewriter::try_new(settings, |bytes: &[u8]| {
        buffer.extend_from_slice(bytes);
    })
    .expect("utf8 is a valid encoding");

    writer.write(html)?;
    writer.end()?;

    Ok(buffer)
}
