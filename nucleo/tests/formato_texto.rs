//! MD y TXT: los párrafos van al motor y el escritor cambia solo los
//! caracteres sustituidos. Todo lo demás, incluidos los `\r\n`, la marca de
//! orden de bytes y los espacios, sale igual byte a byte.

use nucleo::Error;
use nucleo::formatos::texto::TextDocument;
use nucleo::formatos::{Replacement, order_for_writing};

fn replacement(segment: usize, start: usize, end: usize, text: &str) -> Replacement {
    Replacement {
        segment,
        start,
        end,
        text: text.to_owned(),
    }
}

const CONTRACT: &str =
    "Contrato con José Núñez.\r\nFirmado en Valencia.\r\n\r\nCorreo: jose@ejemplo.es\r\n";

#[test]
fn segments_are_the_paragraphs_without_line_endings() {
    let document = TextDocument::from_bytes(CONTRACT.as_bytes()).unwrap();
    assert_eq!(
        document.segments(),
        [
            "Contrato con José Núñez.\r\nFirmado en Valencia.",
            "Correo: jose@ejemplo.es"
        ]
    );
}

#[test]
fn without_replacements_the_output_is_identical() {
    for text in [
        CONTRACT,
        "",
        "\n\n  \n",
        "sin salto final",
        "  espacios al principio y al final  \n\n\n\tTabulador\r\n",
        "\u{FEFF}# Título con marca de orden de bytes\n\n- lista\n",
        "Mezcla\r\nde\nfinales\r\n\r\n\nde línea 😀",
    ] {
        let document = TextDocument::from_bytes(text.as_bytes()).unwrap();
        assert_eq!(document.write(&[]).unwrap(), text.as_bytes(), "{text:?}");
    }
}

#[test]
fn replacements_change_only_their_characters() {
    let document = TextDocument::from_bytes(CONTRACT.as_bytes()).unwrap();
    // Las posiciones son caracteres dentro de cada segmento.
    let output = document
        .write(&[
            replacement(1, 8, 23, "[EMAIL_0001]"),
            replacement(0, 13, 23, "[NOMBRE_PERSONA_0001]"),
            replacement(0, 37, 45, "[LUGAR_0001]"),
        ])
        .unwrap();
    assert_eq!(
        String::from_utf8(output).unwrap(),
        "Contrato con [NOMBRE_PERSONA_0001].\r\nFirmado en [LUGAR_0001].\r\n\r\nCorreo: [EMAIL_0001]\r\n"
    );
}

#[test]
fn replacements_can_touch_each_other_and_reach_the_segment_ends() {
    let document = TextDocument::from_bytes("Ana Gil\n\nañoñú".as_bytes()).unwrap();
    let output = document
        .write(&[
            replacement(0, 0, 3, "[A]"),
            replacement(0, 3, 7, "[B]"),
            replacement(1, 0, 5, "[C]"),
        ])
        .unwrap();
    assert_eq!(output, b"[A][B]\n\n[C]");
}

#[test]
fn the_byte_order_mark_is_kept_but_not_sent_to_the_model() {
    let document = TextDocument::from_bytes("\u{FEFF}Ana Gil\r\n".as_bytes()).unwrap();
    assert_eq!(document.segments(), ["Ana Gil"]);
    let output = document.write(&[replacement(0, 0, 7, "[P]")]).unwrap();
    assert_eq!(output, "\u{FEFF}[P]\r\n".as_bytes());
}

#[test]
fn invalid_replacements_are_rejected() {
    let document = TextDocument::from_bytes("Ana Gil\n\nLuis".as_bytes()).unwrap();
    let cases = [
        vec![replacement(0, 0, 5, "x"), replacement(0, 4, 7, "y")],
        vec![replacement(0, 2, 3, "x"), replacement(0, 0, 7, "y")],
        vec![replacement(0, 0, 3, "x"), replacement(0, 0, 3, "y")],
        vec![replacement(0, 0, 8, "x")],
        vec![replacement(1, 3, 5, "x")],
        vec![replacement(0, 3, 3, "x")],
        vec![replacement(0, 4, 2, "x")],
        vec![replacement(2, 0, 1, "x")],
    ];
    for replacements in cases {
        let error = document.write(&replacements).unwrap_err();
        assert!(
            matches!(error, Error::InvalidReplacement(_)),
            "{replacements:?}: {error:?}"
        );
        assert!(!error.to_string().contains("Ana"), "{error}");
    }
}

#[test]
fn replacements_are_ordered_from_the_last_to_the_first() {
    let replacements = [
        replacement(0, 0, 2, "a"),
        replacement(1, 0, 1, "b"),
        replacement(0, 5, 6, "c"),
        replacement(1, 3, 4, "d"),
    ];
    let ordered = order_for_writing(&replacements, &[10, 10]).unwrap();
    let texts: Vec<&str> = ordered.iter().map(|r| r.text.as_str()).collect();
    assert_eq!(texts, ["d", "b", "c", "a"]);
}

#[test]
fn text_that_is_not_utf8_is_a_clear_error() {
    // «Núñez» en Windows-1252, el formato de muchos TXT antiguos.
    let error = TextDocument::from_bytes(b"N\xfa\xf1ez").unwrap_err();
    assert!(matches!(error, Error::TextNotUtf8), "{error:?}");
    assert!(error.to_string().contains("UTF-8"), "{error}");
}
