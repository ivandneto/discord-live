import { useEffect, useMemo, useState } from "react";
import { geoGraticule10, geoInterpolate, geoOrthographic, geoPath } from "d3-geo";
import { feature } from "topojson-client";
import type { Topology, GeometryCollection } from "topojson-specification";
import terra from "world-atlas/land-110m.json";

type Coord = [number, number]; // [lat, lon]

const LAND = feature(
  terra as unknown as Topology<{ land: GeometryCollection }>,
  (terra as unknown as Topology<{ land: GeometryCollection }>).objects.land,
);
const GRADE = geoGraticule10();

/**
 * Globo em projeção ortográfica com a posição do usuário e da saída,
 * ligadas por um arco de grande círculo. O globo gira devagar até centralizar
 * o meio do caminho.
 */
export function Globo(p: { voce?: Coord | null; saida?: Coord | null; tom: string; tamanho?: number }) {
  const t = p.tamanho ?? 196;
  const r = t / 2 - 4;

  // Centro desejado: ponto médio do arco (ou o usuário, ou o Brasil).
  const alvo = useMemo<[number, number]>(() => {
    const v = p.voce ? ([p.voce[1], p.voce[0]] as [number, number]) : ([-51.9, -14.2] as [number, number]);
    if (!p.saida) return v;
    const s: [number, number] = [p.saida[1], p.saida[0]];
    return geoInterpolate(v, s)(0.5) as [number, number];
  }, [p.voce, p.saida]);

  // Rotação suave até o alvo.
  const [rot, setRot] = useState<[number, number]>(alvo);
  useEffect(() => {
    let quadro = 0;
    const passo = () => {
      setRot((atual) => {
        const d0 = ((alvo[0] - atual[0] + 540) % 360) - 180;
        const d1 = alvo[1] - atual[1];
        if (Math.abs(d0) < 0.2 && Math.abs(d1) < 0.2) return alvo;
        quadro = requestAnimationFrame(passo);
        return [atual[0] + d0 * 0.08, atual[1] + d1 * 0.08];
      });
    };
    quadro = requestAnimationFrame(passo);
    return () => cancelAnimationFrame(quadro);
  }, [alvo]);

  const proj = geoOrthographic()
    .scale(r)
    .translate([t / 2, t / 2])
    .rotate([-rot[0], -rot[1]])
    .clipAngle(90);
  const caminho = geoPath(proj);

  const ponto = (c?: Coord | null) => {
    if (!c) return null;
    const lonlat: [number, number] = [c[1], c[0]];
    // Só desenha se estiver na face visível.
    const visivel = caminho({ type: "Point", coordinates: lonlat });
    return visivel ? proj(lonlat) : null;
  };
  const pv = ponto(p.voce);
  const ps = ponto(p.saida);

  const arco =
    p.voce && p.saida
      ? caminho({
          type: "LineString",
          coordinates: [
            [p.voce[1], p.voce[0]],
            [p.saida[1], p.saida[0]],
          ],
        })
      : null;

  return (
    <svg className={`globo tom-${p.tom}`} width={t} height={t} viewBox={`0 0 ${t} ${t}`} aria-hidden>
      <defs>
        <radialGradient id="brilho" cx="35%" cy="30%" r="75%">
          <stop offset="0%" stopColor="var(--globo-luz)" />
          <stop offset="100%" stopColor="var(--globo-mar)" />
        </radialGradient>
      </defs>
      <circle cx={t / 2} cy={t / 2} r={r + 3} className="globo-halo" />
      <path d={caminho({ type: "Sphere" }) ?? ""} fill="url(#brilho)" className="globo-esfera" />
      <path d={caminho(GRADE) ?? ""} className="globo-grade" />
      <path d={caminho(LAND) ?? ""} className="globo-terra" />
      {arco && <path d={arco} className="globo-arco" />}
      {pv && (
        <g transform={`translate(${pv[0]},${pv[1]})`}>
          <circle r={9} className="globo-pulso" />
          <circle r={4.5} className="globo-voce" />
        </g>
      )}
      {ps && (
        <g transform={`translate(${ps[0]},${ps[1]})`}>
          <circle r={4.5} className="globo-saida" />
        </g>
      )}
    </svg>
  );
}
