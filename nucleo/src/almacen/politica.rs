//! Plazo de caducidad fijado por sistemas (PRD, «Caducidad»): una clave de
//! registro en Windows y un perfil de configuración en Mac. Si hay política,
//! manda ella y el usuario no la puede cambiar.

/// Clave de las políticas en Windows, bajo `HKEY_LOCAL_MACHINE`.
pub const WINDOWS_KEY: &str = r"SOFTWARE\Policies\Cuatroochenta\Anonimizador";
/// Dominio del perfil de configuración en Mac. Un perfil gestionado (Jamf,
/// Intune) se instala en `/Library/Managed Preferences/<dominio>.plist`.
pub const MAC_DOMAIN: &str = "com.cuatroochenta.anonimizador";
/// Valor con los días: un DWORD en Windows, un entero en el perfil de Mac.
pub const VALUE: &str = "DiasCaducidad";
/// Un plazo fuera de 1 a 3650 días se ignora.
pub const MAX_DAYS: u32 = 3650;

/// Los días de la política del sistema, si la hay y es válida.
pub fn retention_policy() -> Option<u32> {
    let days = read();
    let valid = days.and_then(valid_days);
    if days.is_some() && valid.is_none() {
        tracing::warn!(dias = ?days, "la política de caducidad no es válida y se ignora");
    }
    valid
}

/// El plazo, si está entre 1 y [`MAX_DAYS`].
pub fn valid_days(days: u64) -> Option<u32> {
    u32::try_from(days)
        .ok()
        .filter(|d| (1..=MAX_DAYS).contains(d))
}

/// Lo que escribe `defaults read` para un entero: el número y un salto.
pub fn parse_defaults(output: &str) -> Option<u64> {
    output.trim().parse().ok()
}

#[cfg(windows)]
fn read() -> Option<u64> {
    let key = windows_registry::LOCAL_MACHINE.open(WINDOWS_KEY).ok()?;
    key.get_u32(VALUE).ok().map(u64::from)
}

#[cfg(target_os = "macos")]
fn read() -> Option<u64> {
    let plist = format!("/Library/Managed Preferences/{MAC_DOMAIN}.plist");
    if !std::path::Path::new(&plist).exists() {
        return None;
    }
    let output = std::process::Command::new("/usr/bin/defaults")
        .args(["read", plist.trim_end_matches(".plist"), VALUE])
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| parse_defaults(&String::from_utf8_lossy(&output.stdout)))
        .flatten()
}

#[cfg(not(any(windows, target_os = "macos")))]
fn read() -> Option<u64> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_reasonable_days_count() {
        assert_eq!(valid_days(30), Some(30));
        assert_eq!(valid_days(3650), Some(3650));
        assert_eq!(valid_days(0), None);
        assert_eq!(valid_days(3651), None);
        assert_eq!(valid_days(u64::MAX), None);
    }

    #[test]
    fn the_output_of_defaults_is_a_number() {
        assert_eq!(parse_defaults("30\n"), Some(30));
        assert_eq!(parse_defaults("treinta"), None);
    }
}
