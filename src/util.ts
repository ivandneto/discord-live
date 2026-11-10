const REGIOES: Record<string, string> = {
  brazil: "Brasil",
  "buenos-aires": "Buenos Aires",
  santiago: "Santiago",
  "us-east": "EUA (Leste)",
  "us-central": "EUA (Central)",
  "us-south": "EUA (Sul)",
  "us-west": "EUA (Oeste)",
  atlanta: "Atlanta",
  newark: "Newark",
  seattle: "Seattle",
  rotterdam: "Roterdã",
  amsterdam: "Amsterdã",
  paris: "Paris",
  bucharest: "Bucareste",
  warsaw: "Varsóvia",
  helsinki: "Helsinque",
  "us-east-1": "EUA (Leste)",
  tokyo: "Tóquio",
  "st-pete": "São Petersburgo",
  russia: "Rússia",
  "tel-aviv": "Tel Aviv",
  madrid: "Madri",
  milan: "Milão",
  frankfurt: "Frankfurt",
  london: "Londres",
  stockholm: "Estocolmo",
  japan: "Japão",
  singapore: "Singapura",
  hongkong: "Hong Kong",
  sydney: "Sydney",
  india: "Índia",
  southafrica: "África do Sul",
  "south-korea": "Coreia do Sul",
  dubai: "Dubai",
};

export function nomeRegiao(r: string | null | undefined): string {
  if (!r) return "—";
  return REGIOES[r] ?? r.charAt(0).toUpperCase() + r.slice(1).replace(/-/g, " ");
}

/** Converte o código do país (ex.: CL) em bandeira emoji. */
export function bandeira(pais: string | null | undefined): string {
  if (!pais || pais.length !== 2 || pais === "XX" || pais === "T1") return "🌐";
  const A = 0x1f1e6;
  const c = pais.toUpperCase();
  return String.fromCodePoint(A + c.charCodeAt(0) - 65, A + c.charCodeAt(1) - 65);
}

const nomesPaises = (() => {
  try {
    return new Intl.DisplayNames(["pt-BR"], { type: "region" });
  } catch {
    return null;
  }
})();

export function nomePais(pais: string | null | undefined): string {
  if (!pais) return "—";
  try {
    return nomesPaises?.of(pais.toUpperCase()) ?? pais;
  } catch {
    return pais;
  }
}

export function haQuanto(ms: number, agora = Date.now()): string {
  const s = Math.max(0, Math.round((agora - ms) / 1000));
  if (s < 5) return "agora";
  if (s < 60) return `há ${s} s`;
  const m = Math.round(s / 60);
  if (m < 60) return `há ${m} min`;
  const h = Math.round(m / 60);
  if (h < 24) return `há ${h} h`;
  return `há ${Math.round(h / 24)} d`;
}
