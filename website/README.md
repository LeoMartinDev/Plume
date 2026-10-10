# speakplume.com

The Plume website: a static [Astro](https://astro.build) site, deployed to Cloudflare Pages.

## Develop

```sh
cd website
npm install
npm run dev      # http://localhost:4321
npm run build    # outputs to dist/
```

Requires Node 22.12 or newer.

## Structure

- `src/components/Landing.astro` composes the page; each section is its own component.
- `src/components/VoicePill.astro` is the voice pill (`idle`, `listening`, `transcribing`, `inserted`).
- `src/lib/demo.ts` drives the looping demos (hero window, "works in every app" card). They pause
  off screen and stay static when the visitor prefers reduced motion.
- `src/i18n/strings.ts` holds all copy, in English (`/`) and French (`/fr/`). Edit text there.
- `src/styles/global.css` holds the color tokens for light and dark themes.
- `src/lib/release.ts` fetches the latest GitHub release at build time to link each installer
  (`.dmg`, `.exe`, `.deb`). If GitHub is unreachable, links fall back to the latest release page.

The theme follows the system setting, and the toggle in the nav saves a manual choice.
The hero download button detects the visitor's OS and links straight to its installer.

## Deploy (Cloudflare Pages)

The Cloudflare Worker `plume` (static assets only, see `wrangler.jsonc`) is connected to this
GitHub repo through Workers Builds:

- Root directory: `website`
- Build command: `npm run build`
- Deploy command: `npx wrangler deploy`
- Non-production branch deploy command: `npx wrangler versions upload` (preview URLs)
- Build watch paths → include `website/*`, so app-only commits don't rebuild the site
- Node version: from `website/.nvmrc` (22; Astro needs 22.12+)

When a GitHub release is published, `.github/workflows/website.yml` calls a Cloudflare
deploy hook so the site rebuilds and the download buttons point at the new installers.
Setup: Worker → Settings → Build → Deploy hooks → create one for `main`, then save its URL
as the repo secret `CLOUDFLARE_DEPLOY_HOOK`.

The app CI (`ci.yml`) ignores `website/**`, so website-only changes don't trigger app builds.

Custom domain: Worker → Settings → Domains & Routes → add `speakplume.com` (and `www.speakplume.com`).
