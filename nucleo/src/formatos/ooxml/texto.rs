//! Ayudas para escribir MD y TXT desde PPTX y XLSX.

/// Escapa lo que al principio de un párrafo Markdown leería como título,
/// cita, lista o tabla.
pub(crate) fn escape_start(text: &str) -> String {
    if text.starts_with(['#', '>', '-', '+', '*', '=', '|', '`', '~']) {
        return format!("\\{text}");
    }
    let digits = text.chars().take_while(char::is_ascii_digit).count();
    if digits > 0 && text[digits..].starts_with(['.', ')']) {
        return format!("{}\\{}", &text[..digits], &text[digits..]);
    }
    text.to_owned()
}

/// Tabla Markdown con la primera fila como cabecera. Todas las filas tienen
/// tantas celdas como la más larga.
pub(crate) fn markdown_table(rows: &[Vec<String>]) -> String {
    let columns = rows.iter().map(Vec::len).max().unwrap_or(0);
    if columns == 0 {
        return String::new();
    }
    let line = |cells: Vec<String>| format!("| {} |", cells.join(" | "));
    let mut lines = Vec::new();
    for (index, row) in rows.iter().enumerate() {
        let cells = (0..columns)
            .map(|c| {
                row.get(c)
                    .map(|text| text.replace('|', "\\|").replace('\n', " "))
                    .unwrap_or_default()
            })
            .collect();
        lines.push(line(cells));
        if index == 0 {
            lines.push(line(vec!["---".to_owned(); columns]));
        }
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markdown_starts_are_escaped() {
        assert_eq!(escape_start("# no es título"), "\\# no es título");
        assert_eq!(escape_start("1. no es lista"), "1\\. no es lista");
        assert_eq!(escape_start("normal"), "normal");
    }

    #[test]
    fn tables_are_padded_and_escaped() {
        let rows = vec![
            vec!["Nombre".to_owned(), "Nota".to_owned()],
            vec!["a|b".to_owned()],
        ];
        assert_eq!(
            markdown_table(&rows),
            "| Nombre | Nota |\n| --- | --- |\n| a\\|b |  |"
        );
        assert_eq!(markdown_table(&[]), "");
    }
}
