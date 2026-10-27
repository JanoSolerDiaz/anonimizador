# Corpus sintético de detección

Textos inventados para medir la detección de datos personales con
`cli evaluar tests/corpus/texto`. Ningún dato pertenece a una persona real.
Los DNI, NIE, IBAN, números de tarjeta, CIF, números de la Seguridad Social,
NIR franceses y números del NHS británico tienen el dígito de control
correcto, para que se parezcan a los de verdad.

Hay 8 textos por idioma (castellano `es`, catalán `ca`, inglés `en` y francés
`fr`) y un texto largo en castellano (`es-09-largo`). El texto largo es un
solo párrafo de unos 6.000 caracteres, así que el motor tiene que trocearlo
en varias ventanas y algunas entidades caen en las zonas de solape. El
octavo texto de cada idioma no tiene datos personales y sirve para contar
detecciones de más.

## Formato

Cada `nombre.txt` lleva al lado un `nombre.json`:

```json
{
  "entidades": [
    { "tipo": "NOMBRE_PERSONA", "texto": "Marta Lozano Ibáñez" }
  ]
}
```

Las posiciones no se escriben: el evaluador busca el texto. Cada aparición
como palabra completa cuenta como una entidad, salvo las que quedan dentro
de otra entidad más larga (la «Zaragoza» de una dirección postal forma
parte de la dirección). Si un texto del JSON no aparece, el evaluador se
detiene y lo dice.

## Criterios de anotación

- **Nombres**: sin tratamientos ni títulos (`Dr.`, `Sra.`, `Mme`, `Mr`). Si
  luego aparece solo el nombre o el apellido, también se anota.
- **Cargos**: el sintagma que nombra el puesto (`jefe de servicio`,
  `técnico de mantenimiento`), no los papeles del documento como
  «arrendador» o «paciente».
- **Direcciones**: de la calle al municipio, con el código postal. Una
  ciudad sola es `LUGAR`.
- **Fechas**: solo la de nacimiento. El resto de fechas no son datos
  personales para el modelo.
- **Importes**: la cifra con la moneda (`780 euros`, `£950`).
- **Organizaciones**: sin puntos finales de abreviatura. El motor siempre
  recorta un punto al final de una entidad, así que se escribe `SL` o `Inc`
  sin punto para no penalizar algo que no es de detección.
- **Categorías especiales** (salud, religión, sindicato, ideología,
  orientación sexual, origen étnico): la palabra o expresión mínima que
  revela el dato (`diabetes tipo 2`, `UGT`, `bisexual`, `gitana`).

Estos criterios son míos y discutibles. Por eso el evaluador da, además de
la coincidencia estricta, la columna «Tapadas»: cuántas entidades quedan
ocultas por completo aunque el modelo haya puesto los bordes o el tipo de
otra manera.
