/** @type {import('tailwindcss').Config} */
// Tailwind v3, matching the `tailwindcss` in nixpkgs and the pin in Trunk.toml.
// Kept in sync with the three type roles in docs/how-to/build-and-run.md.
module.exports = {
  // Classes live in the Leptos view! macros, so Tailwind has to scan the Rust
  // source. Anything constructed at runtime (format!("text-{}", colour)) will
  // NOT be found — build such classes from a match returning whole literals.
  content: ["./src/**/*.rs", "./src/app/index.html"],
  theme: {
    extend: {
      fontFamily: {
        // Assigned by role, not by classification: display is set once and read
        // as a shape; body carries sentences; mono carries anything that is a
        // *value* — run IDs, metrics, counts, timestamps, tags, code.
        display: ["Space Grotesk Variable", "ui-sans-serif", "system-ui", "sans-serif"],
        body: ["Nunito Variable", "ui-sans-serif", "system-ui", "-apple-system", "Segoe UI", "sans-serif"],
        // `sans` points at the body face so stray font-sans utilities cannot
        // diverge from what <body> actually uses.
        sans: ["Nunito Variable", "ui-sans-serif", "system-ui", "-apple-system", "Segoe UI", "sans-serif"],
        mono: ["Cascadia Code Variable", "ui-monospace", "SFMono-Regular", "Menlo", "monospace"],
      },
    },
  },
  // Bundled in the standalone tailwindcss CLI (both trunk's download and the
  // nixpkgs build), so it needs no npm install. The dashboard renders project
  // READMEs with `prose` classes — these were silently unstyled the whole time
  // the plain Play CDN was used, since that ships no plugins.
  plugins: [require("@tailwindcss/typography")],
};
