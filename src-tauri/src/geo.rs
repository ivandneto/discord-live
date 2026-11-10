//! Geografia aproximada para escolher proxies perto do usuário.
//!
//! - Onde o usuário está: o trace da Cloudflare (sem chave, sem cadastro) dá o
//!   país (`loc`) e o data center que atendeu (`colo`, código de aeroporto).
//!   O IP do usuário não vai para nenhum serviço de geolocalização novo.
//! - Onde o proxy está: as listas do monosans trazem latitude/longitude; as do
//!   proxifly trazem o país (usamos o centro aproximado do país).

/// Centro aproximado (lat, lon) de países que aparecem nas listas de proxies.
const PAISES: &[(&str, f64, f64)] = &[
    // Américas
    ("BR", -14.2, -51.9), ("AR", -38.4, -63.6), ("CL", -35.7, -71.5), ("UY", -32.5, -55.8),
    ("PY", -23.4, -58.4), ("BO", -16.3, -63.6), ("PE", -9.2, -75.0), ("CO", 4.6, -74.3),
    ("EC", -1.8, -78.2), ("VE", 6.4, -66.6), ("MX", 23.6, -102.6), ("US", 39.8, -98.6),
    ("CA", 56.1, -106.3), ("PA", 8.5, -80.8), ("CR", 9.7, -83.8), ("GT", 15.8, -90.2),
    ("DO", 18.7, -70.2), ("HN", 15.2, -86.2), ("SV", 13.8, -88.9), ("NI", 12.9, -85.2),
    ("PR", 18.2, -66.6), ("TT", 10.7, -61.2), ("JM", 18.1, -77.3), ("CU", 21.5, -77.8),
    // Europa
    ("PT", 39.4, -8.2), ("ES", 40.5, -3.7), ("FR", 46.2, 2.2), ("GB", 55.4, -3.4),
    ("IE", 53.4, -8.2), ("NL", 52.1, 5.3), ("BE", 50.5, 4.5), ("DE", 51.2, 10.5),
    ("IT", 41.9, 12.6), ("CH", 46.8, 8.2), ("AT", 47.5, 14.6), ("PL", 51.9, 19.1),
    ("CZ", 49.8, 15.5), ("SE", 60.1, 18.6), ("NO", 60.5, 8.5), ("FI", 61.9, 25.7),
    ("DK", 56.3, 9.5), ("RO", 45.9, 24.97), ("BG", 42.7, 25.5), ("GR", 39.1, 21.8),
    ("HU", 47.2, 19.5), ("UA", 48.4, 31.2), ("RU", 55.8, 37.6), ("TR", 39.0, 35.2),
    ("RS", 44.0, 21.0), ("LT", 55.2, 23.9), ("LV", 56.9, 24.6), ("EE", 58.6, 25.0),
    ("MD", 47.4, 28.4), ("BY", 53.7, 27.95), ("SK", 48.7, 19.7), ("HR", 45.1, 15.2),
    // África e Oriente Médio
    ("ZA", -30.6, 22.9), ("NG", 9.1, 8.7), ("EG", 26.8, 30.8), ("KE", -0.02, 37.9),
    ("MA", 31.8, -7.1), ("IL", 31.0, 34.9), ("AE", 23.4, 53.8), ("SA", 23.9, 45.1),
    ("IR", 32.4, 53.7), ("IQ", 33.2, 43.7),
    // Ásia e Oceania
    ("IN", 20.6, 79.0), ("CN", 35.9, 104.2), ("HK", 22.3, 114.2), ("TW", 23.7, 121.0),
    ("JP", 36.2, 138.3), ("KR", 35.9, 127.8), ("SG", 1.35, 103.8), ("MY", 4.2, 102.0),
    ("TH", 15.9, 101.0), ("VN", 14.1, 108.3), ("ID", -0.8, 113.9), ("PH", 12.9, 121.8),
    ("BD", 23.7, 90.4), ("PK", 30.4, 69.3), ("KZ", 48.0, 66.9), ("AU", -25.3, 133.8),
    ("NZ", -40.9, 174.9),
];

/// Data centers da Cloudflare (código IATA) mais comuns no Brasil e arredores.
const COLOS: &[(&str, f64, f64)] = &[
    ("GRU", -23.43, -46.47), ("GIG", -22.81, -43.25), ("BSB", -15.87, -47.92),
    ("CNF", -19.63, -43.97), ("POA", -29.99, -51.17), ("CWB", -25.53, -49.18),
    ("FLN", -27.67, -48.55), ("FOR", -3.78, -38.53), ("REC", -8.13, -34.92),
    ("SSA", -12.91, -38.33), ("BEL", -1.38, -48.48), ("MAO", -3.04, -60.05),
    ("VCP", -23.0, -47.13), ("SJP", -20.82, -49.4), ("ITJ", -26.88, -48.65),
    ("QWJ", -22.74, -47.33), ("BNU", -26.92, -49.07), ("JOI", -26.22, -48.8),
    ("RAO", -21.13, -47.77), ("UDI", -18.88, -48.22), ("XAP", -27.13, -52.66),
    ("NVT", -26.88, -48.65), ("GYN", -16.63, -49.22), ("CGB", -15.65, -56.12),
    ("CGR", -20.47, -54.67), ("NAT", -5.77, -35.36), ("AJU", -10.98, -37.07),
    ("MCZ", -9.51, -35.79), ("SLZ", -2.59, -44.23), ("THE", -5.06, -42.82),
    ("PMW", -10.29, -48.36), ("PVH", -8.71, -63.9), ("CAW", -21.7, -41.3),
    ("EZE", -34.82, -58.54), ("SCL", -33.39, -70.79), ("ASU", -25.24, -57.52),
    ("MVD", -34.84, -56.03), ("LIM", -12.02, -77.11), ("BOG", 4.7, -74.15),
    ("MIA", 25.79, -80.29),
];

pub fn centro_pais(codigo: &str) -> Option<(f64, f64)> {
    PAISES
        .iter()
        .find(|(c, _, _)| c.eq_ignore_ascii_case(codigo))
        .map(|&(_, la, lo)| (la, lo))
}

pub fn colo(codigo: &str) -> Option<(f64, f64)> {
    COLOS
        .iter()
        .find(|(c, _, _)| c.eq_ignore_ascii_case(codigo))
        .map(|&(_, la, lo)| (la, lo))
}

/// Distância em km (haversine).
pub fn distancia_km(a: (f64, f64), b: (f64, f64)) -> f64 {
    let r = 6371.0;
    let (la1, lo1, la2, lo2) = (a.0.to_radians(), a.1.to_radians(), b.0.to_radians(), b.1.to_radians());
    let h = ((la2 - la1) / 2.0).sin().powi(2) + la1.cos() * la2.cos() * ((lo2 - lo1) / 2.0).sin().powi(2);
    2.0 * r * h.sqrt().asin()
}

#[derive(Clone, Debug, PartialEq)]
pub struct Local {
    pub pais: String,
    pub colo: Option<String>,
    pub coord: (f64, f64),
}

/// Lê o trace da Cloudflare (`loc=BR`, `colo=GIG`).
pub fn local_do_trace(trace: &str) -> Option<Local> {
    let campo = |k: &str| {
        trace
            .lines()
            .find_map(|l| l.strip_prefix(k).and_then(|r| r.strip_prefix('=')))
            .map(|v| v.trim().to_string())
    };
    let pais = campo("loc")?.to_ascii_uppercase();
    let c = campo("colo");
    let coord = c.as_deref().and_then(colo).or_else(|| centro_pais(&pais))?;
    Some(Local { pais, colo: c, coord })
}

#[derive(Clone, Debug, PartialEq)]
pub struct CandidatoGeo {
    pub host: String,
    pub porta: u16,
    pub pais: Option<String>,
    pub coord: Option<(f64, f64)>,
}

fn ip_publico(h: &str) -> Option<String> {
    let ip: std::net::Ipv4Addr = h.parse().ok()?;
    if ip.is_private() || ip.is_loopback() || ip.is_unspecified() || ip.is_link_local() || ip.is_broadcast() {
        return None;
    }
    Some(ip.to_string())
}

/// `proxies.json` do monosans: só SOCKS5, com latitude/longitude.
pub fn ler_monosans(json: &str) -> Vec<CandidatoGeo> {
    let Ok(serde_json::Value::Array(v)) = serde_json::from_str::<serde_json::Value>(json) else {
        return vec![];
    };
    v.iter()
        .filter(|p| p["protocol"].as_str() == Some("socks5"))
        .filter(|p| p["username"].is_null())
        .filter_map(|p| {
            let host = ip_publico(p["host"].as_str()?)?;
            let porta = u16::try_from(p["port"].as_u64()?).ok().filter(|&x| x != 0)?;
            let g = &p["geolocation"];
            let pais = g["country"]["iso_code"].as_str().map(str::to_string);
            let coord = match (g["location"]["latitude"].as_f64(), g["location"]["longitude"].as_f64()) {
                (Some(la), Some(lo)) => Some((la, lo)),
                _ => pais.as_deref().and_then(centro_pais),
            };
            Some(CandidatoGeo { host, porta, pais, coord })
        })
        .collect()
}

/// `data.json` do proxifly: país, sem coordenadas.
pub fn ler_proxifly(json: &str) -> Vec<CandidatoGeo> {
    let Ok(serde_json::Value::Array(v)) = serde_json::from_str::<serde_json::Value>(json) else {
        return vec![];
    };
    v.iter()
        .filter(|p| p["protocol"].as_str() == Some("socks5"))
        .filter_map(|p| {
            let host = ip_publico(p["ip"].as_str()?)?;
            let porta = u16::try_from(p["port"].as_u64()?).ok().filter(|&x| x != 0)?;
            let pais = p["geolocation"]["country"]
                .as_str()
                .filter(|c| c.len() == 2 && *c != "ZZ")
                .map(|c| c.to_ascii_uppercase());
            let coord = pais.as_deref().and_then(centro_pais);
            Some(CandidatoGeo { host, porta, pais, coord })
        })
        .collect()
}

/// Texto simples, `ip:porta` por linha, sem geografia.
pub fn ler_texto(texto: &str) -> Vec<CandidatoGeo> {
    texto
        .lines()
        .filter_map(|l| {
            let l = l.trim();
            let l = l.strip_prefix("socks5://").unwrap_or(l);
            let (h, p) = l.split_once(':')?;
            let host = ip_publico(h)?;
            let porta: u16 = p.trim().parse().ok().filter(|&x| x != 0)?;
            Some(CandidatoGeo { host, porta, pais: None, coord: None })
        })
        .collect()
}

/// Ordena do mais perto ao mais longe; quem não tem posição vai para o fim.
/// Tira os do país do usuário: saindo pelo mesmo país, a região não muda.
pub fn ordenar_por_proximidade(v: &mut Vec<CandidatoGeo>, local: Option<&Local>) {
    if let Some(l) = local {
        v.retain(|c| c.pais.as_deref() != Some(l.pais.as_str()));
        v.sort_by(|a, b| {
            let da = a.coord.map(|c| distancia_km(l.coord, c)).unwrap_or(f64::MAX);
            let db = b.coord.map(|c| distancia_km(l.coord, c)).unwrap_or(f64::MAX);
            da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
        });
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn le_o_local_pelo_trace() {
        let l = local_do_trace("fl=1\nloc=BR\ncolo=GIG\n").unwrap();
        assert_eq!(l.pais, "BR");
        assert_eq!(l.coord, (-22.81, -43.25));
        // Colo desconhecido: cai para o centro do país.
        let l = local_do_trace("loc=CL\ncolo=XYZ\n").unwrap();
        assert_eq!(l.coord, (-35.7, -71.5));
        assert!(local_do_trace("nada").is_none());
    }

    #[test]
    fn distancia_aproximada() {
        // Rio a Buenos Aires: ~2000 km.
        let d = distancia_km((-22.81, -43.25), (-34.82, -58.54));
        assert!((1800.0..2200.0).contains(&d), "{d}");
    }

    #[test]
    fn mais_perto_primeiro_e_sem_o_proprio_pais() {
        let local = local_do_trace("loc=BR\ncolo=GIG\n").unwrap();
        let mut v = vec![
            CandidatoGeo { host: "1.1.1.1".into(), porta: 1, pais: Some("DE".into()), coord: centro_pais("DE") },
            CandidatoGeo { host: "2.2.2.2".into(), porta: 1, pais: None, coord: None },
            CandidatoGeo { host: "3.3.3.3".into(), porta: 1, pais: Some("AR".into()), coord: centro_pais("AR") },
            CandidatoGeo { host: "4.4.4.4".into(), porta: 1, pais: Some("BR".into()), coord: centro_pais("BR") },
            CandidatoGeo { host: "5.5.5.5".into(), porta: 1, pais: Some("US".into()), coord: centro_pais("US") },
        ];
        ordenar_por_proximidade(&mut v, Some(&local));
        let ordem: Vec<_> = v.iter().map(|c| c.host.as_str()).collect();
        assert_eq!(ordem, ["3.3.3.3", "5.5.5.5", "1.1.1.1", "2.2.2.2"]);
    }

    #[test]
    fn le_os_jsons_das_listas() {
        let m = r#"[{"protocol":"socks5","username":null,"host":"88.1.2.3","port":1080,
            "geolocation":{"country":{"iso_code":"RU"},"location":{"latitude":59.9,"longitude":30.3}}},
            {"protocol":"socks4","username":null,"host":"9.9.9.9","port":1080,"geolocation":{}}]"#;
        let v = ler_monosans(m);
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].pais.as_deref(), Some("RU"));
        assert_eq!(v[0].coord, Some((59.9, 30.3)));

        let p = r#"[{"protocol":"socks5","ip":"200.1.2.3","port":443,"geolocation":{"country":"CL","city":"x"}},
            {"protocol":"socks5","ip":"10.0.0.1","port":1,"geolocation":{"country":"CL"}}]"#;
        let v = ler_proxifly(p);
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].pais.as_deref(), Some("CL"));
        assert_eq!(v[0].coord, centro_pais("CL"));
    }
}
