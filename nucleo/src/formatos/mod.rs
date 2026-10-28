//! Un extractor y un escritor por formato. El extractor devuelve el texto y un
//! mapa de dónde viene cada carácter; el escritor aplica los cambios sobre el
//! archivo original usando ese mapa.

pub mod docx;
pub mod pdf;
pub mod salida;
pub mod texto;
