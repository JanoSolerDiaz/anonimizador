// Textos de la interfaz: nombres de los tipos de datos, de los formatos y de
// cada fase. Todo en español y sin jerga.

import type { Phase, Source, Warnings } from "./api";

/** Nombre de cada tipo de dato (`nucleo/config/tipos.json`). */
const KINDS: Record<string, string> = {
  NOMBRE_PERSONA: "Nombres de personas",
  ALIAS_PERSONA: "Alias",
  ID_DOCUMENTO_PERSONA: "Documentos de identidad",
  CARACTERISTICA_PERSONA: "Características personales",
  CARGO: "Cargos",
  FECHA_NACIMIENTO: "Fechas de nacimiento",
  NOMBRE_PROPIO: "Otros nombres propios",
  EMAIL: "Correos electrónicos",
  TELEFONO: "Teléfonos",
  CONTACTO: "Otros contactos",
  DIRECCION_POSTAL: "Direcciones",
  LUGAR: "Lugares",
  COORDENADAS: "Coordenadas",
  IDENTIFICADOR_DOCUMENTO: "Identificadores de documentos",
  REFERENCIA: "Referencias",
  ORGANIZACION: "Organizaciones",
  CIF: "CIF",
  ANTECEDENTES: "Antecedentes",
  TARJETA_BANCARIA: "Tarjetas bancarias",
  CVV_TARJETA: "Códigos de tarjeta",
  CUENTA_BANCARIA: "Cuentas bancarias",
  IMPORTE: "Importes",
  USUARIO: "Usuarios",
  CLAVE_SECRETO: "Contraseñas y claves",
  DISPOSITIVO: "Dispositivos",
  IP: "Direcciones IP",
  ENLACE: "Enlaces",
  VEHICULO: "Vehículos",
  DATOS_SALUD: "Datos de salud",
  DATOS_BIOMETRIA: "Datos biométricos",
  ORIGEN_ETNICO: "Origen étnico",
  IDEOLOGIA: "Ideología",
  RELIGION: "Religión",
  ORIENTACION_SEXUAL: "Orientación sexual",
  SINDICATO: "Afiliación sindical",
};

export function kindLabel(kind: string): string {
  return KINDS[kind] ?? kind.toLowerCase().replaceAll("_", " ");
}

const FORMATS: Record<string, string> = {
  pdf: "PDF",
  docx: "Word (DOCX)",
  pptx: "PowerPoint (PPTX)",
  xlsx: "Excel (XLSX)",
  md: "Markdown (MD)",
  txt: "Texto (TXT)",
};

export function formatLabel(format: string): string {
  return FORMATS[format] ?? format.toUpperCase();
}

export function sourceLabel(source: Source): string {
  return source.kind === "store" ? "Mis documentos" : `Diccionario «${source.name}»`;
}

/** Fase en curso en una palabra o dos; `null` si ya ha terminado. */
export function phaseLabel(phase: Phase): string | null {
  switch (phase.state) {
    case "queued":
      return "En cola";
    case "reading":
      return "Leyendo";
    case "analyzing":
      return `Analizando ${Math.round(phase.fraction * 100)} %`;
    case "writing":
      return "Escribiendo";
    case "verifying":
      return "Comprobando";
    default:
      return null;
  }
}

/** Avisos de un archivo anonimizado, cada uno en una frase. */
export function warningLines(warnings: Warnings): string[] {
  const lines: string[] = [];
  const plural = (n: number, one: string, many: string) => (n === 1 ? one : many);
  if (warnings.images > 0) {
    lines.push(
      `El documento tiene ${warnings.images} ${plural(warnings.images, "imagen", "imágenes")}. Su contenido no se ha revisado.`,
    );
  }
  if (warnings.objects > 0) {
    lines.push(
      `El documento tiene ${warnings.objects} ${plural(warnings.objects, "objeto incrustado", "objetos incrustados")}. Su contenido no se ha revisado.`,
    );
  }
  if (warnings.attachments > 0) {
    lines.push(
      `El PDF tenía ${warnings.attachments} ${plural(warnings.attachments, "archivo adjunto", "archivos adjuntos")}. Se han quitado de la salida.`,
    );
  }
  if (warnings.links > 0) {
    lines.push(
      `Se han quitado ${warnings.links} ${plural(warnings.links, "enlace que llevaba", "enlaces que llevaban")} un dato en su destino.`,
    );
  }
  return lines;
}
