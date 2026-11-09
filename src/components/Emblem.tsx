/**
 * The TEMISZ emblem, as on the website: a cemetery reduced to Bauhaus forms
 * (gate pillar, setting sun, cypresses) with an Art Deco sunburst and stepped
 * base. `onDark` recolours it for the deep cobalt panel of the login screen.
 */
export function Emblem({ onDark = false, className = "" }: { onDark?: boolean; className?: string }) {
  const c = onDark
    ? { pillar: "#b8382a", arc: "#ededea", cypress: "#ededea", steps: "var(--c-rail)", ground: "#ededea" }
    : { pillar: "#1e3f8c", arc: "#b8382a", cypress: "#16181d", steps: "var(--c-limestone)", ground: "#16181d" };
  return (
    <svg viewBox="0 0 520 440" aria-hidden focusable="false" className={className}>
      <defs>
        <clipPath id="emblem-above">
          <rect x="0" y="0" width="520" height="420" />
        </clipPath>
      </defs>
      <path
        d="M330 300 330 40M330 300 263 49M330 300 200 75M330 300 146 116M330 300 105 170M330 300 397 49M330 300 460 75M330 300 514 116"
        fill="none"
        stroke="#a8873a"
        strokeWidth="1.4"
      />
      <g clipPath="url(#emblem-above)">
        <circle cx="330" cy="300" r="118" fill="#dda22c" />
      </g>
      <rect x="40" y="120" width="150" height="300" fill={c.pillar} />
      <path d="M190 420H60A130 130 0 0 1 190 290z" fill={c.arc} />
      <path d="M402 420 428 196 454 420zM458 420 476 280 494 420z" fill={c.cypress} />
      <path d="M196 420V398H464V420ZM218 398V380H442V398ZM240 380V364H420V380Z" fill={c.steps} stroke="#a8873a" strokeWidth="2" />
      <path d="M0 420H520" stroke={c.ground} strokeWidth="3" />
    </svg>
  );
}
