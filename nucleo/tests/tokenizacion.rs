//! Tokenización con los tokenizadores reales de v1 y v2.
//!
//! Lo que importa es que cada carácter de cada palabra quede cubierto por
//! algún token con su posición exacta, porque esas posiciones son las que se
//! usan para tapar el dato en el documento.

use nucleo::Error;
use nucleo::motor::texto::split_words;
use nucleo::motor::tokenizacion::{TextTokenizer, Token, assign_words};
use nucleo::recursos::Resources;

const MODELS: [&str; 2] = ["modelo", "modelo-v2"];

/// Dobles espacios, tabulador, espacio de no separación, salto de línea,
/// acentos compuestos y descompuestos, ñ, ele geminada catalana, ligadura,
/// emoji y delimitadores.
const TRICKY: &str =
    "Hola  Juan Pérez, (Madrid)\tl·l Muñoz\u{a0}Pe\u{301}rez 😀 ﬁn.\nAna [ana@ejemplo.es]";

fn load(name: &str) -> TextTokenizer {
    let resources = Resources::locate().expect("no se encuentra la carpeta recursos/");
    TextTokenizer::load(&resources.model(name).tokenizer_path())
        .unwrap_or_else(|e| panic!("{name}: {e}"))
}

fn is_separator(c: char) -> bool {
    c.is_whitespace() || ",;|\"<>()[]{}*^".contains(c)
}

/// Comprueba que los offsets están dentro del texto y nunca retroceden, que
/// cada palabra está cubierta entera por sus tokens y que los tokens sin
/// palabra solo contienen espacios o delimitadores.
fn check_alignment(name: &str, text: &str, tokens: &[Token]) {
    let chars: Vec<char> = text.chars().collect();
    // Un carácter puede dar varios tokens con la misma posición: v1 convierte
    // la ligadura «ﬁ» en «f» + «i», y v2 parte el tabulador en dos.
    let mut previous = (0, 0);
    for t in tokens {
        assert!(
            t.start < t.end && t.end <= chars.len(),
            "{name}: {t:?} fuera del texto"
        );
        assert!(
            t.start >= previous.0 && t.end >= previous.1,
            "{name}: {t:?} retrocede respecto al token anterior"
        );
        previous = (t.start, t.end);
    }

    let words = split_words(text);
    let owners = assign_words(tokens, &words);
    let mut covered = vec![false; chars.len()];
    for (t, owner) in tokens.iter().zip(&owners) {
        match owner {
            Some(_) => covered[t.start..t.end].fill(true),
            None => assert!(
                chars[t.start..t.end].iter().all(|&c| is_separator(c)),
                "{name}: el token {t:?} tiene texto y no pertenece a ninguna palabra"
            ),
        }
    }
    for w in &words {
        let word: String = chars[w.start..w.end].iter().collect();
        assert!(
            covered[w.start..w.end].iter().all(|&c| c),
            "{name}: la palabra {word:?} no está cubierta entera por sus tokens"
        );
    }
}

#[test]
fn loading_a_missing_tokenizer_is_a_model_load_error() {
    let dir = tempfile::tempdir().unwrap();
    let result = TextTokenizer::load(&dir.path().join("tokenizer.json"));
    assert!(matches!(result, Err(Error::ModelLoad(_))));
}

#[test]
fn loading_an_invalid_tokenizer_is_a_model_load_error() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("tokenizer.json");
    std::fs::write(&path, "{\"no\": \"es un tokenizador\"}").unwrap();
    assert!(matches!(
        TextTokenizer::load(&path),
        Err(Error::ModelLoad(_))
    ));
}

#[test]
#[ignore = "necesita los modelos descargados en recursos/"]
fn tricky_text_keeps_exact_offsets_in_both_models() {
    for name in MODELS {
        let tokens = load(name).encode(TRICKY).unwrap();
        check_alignment(name, TRICKY, &tokens);
    }
}

#[test]
#[ignore = "necesita los modelos descargados en recursos/"]
fn long_text_is_not_truncated() {
    // v1 trae el truncado a 512 activado en su tokenizer.json.
    let text = "Juan Pérez vive en la calle Mayor 5 de Madrid. ".repeat(100);
    for name in MODELS {
        let tokens = load(name).encode(&text).unwrap();
        assert!(tokens.len() > 600, "{name}: solo {} tokens", tokens.len());
        // v2 convierte también el espacio final en un token; lo que importa
        // es llegar al final de la última palabra.
        let last_word = split_words(&text).last().unwrap().end;
        assert!(
            tokens.last().unwrap().end >= last_word,
            "{name}: el último token no llega al final"
        );
        check_alignment(name, &text, &tokens);
    }
}

#[test]
#[ignore = "necesita los modelos descargados en recursos/"]
fn encode_adds_no_special_tokens() {
    for name in MODELS {
        let tokenizer = load(name);
        let tokens = tokenizer.encode("Juan").unwrap();
        let specials: Vec<u32> = tokenizer
            .prefix()
            .iter()
            .chain(tokenizer.suffix())
            .copied()
            .collect();
        assert!(
            tokens.iter().all(|t| !specials.contains(&t.id)),
            "{name}: {tokens:?}"
        );
    }
}

#[test]
#[ignore = "necesita los modelos descargados en recursos/"]
fn special_tokens_come_from_each_tokenizer() {
    // v1 usa <s> … </s> (0 y 2); v2 usa <bos> … <eos> (2 y 1).
    let v1 = load("modelo");
    assert_eq!((v1.prefix(), v1.suffix()), (&[0][..], &[2][..]));
    let v2 = load("modelo-v2");
    assert_eq!((v2.prefix(), v2.suffix()), (&[2][..], &[1][..]));
}

#[test]
#[ignore = "necesita los modelos descargados en recursos/"]
fn empty_and_blank_text_give_no_tokens_with_content() {
    for name in MODELS {
        let tokenizer = load(name);
        assert!(tokenizer.encode("").unwrap().is_empty(), "{name}");
        let blank = " \n\t ";
        let tokens = tokenizer.encode(blank).unwrap();
        check_alignment(name, blank, &tokens);
    }
}
