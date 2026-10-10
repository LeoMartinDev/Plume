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
- `src/i18n/strings.ts` holds all copy, in English (`/`) and French (`/fr/`). Edit text there.
- `src/styles/global.css` holds the color tokens for light and dark themes.
- `src/lib/release.ts` fetches the latest GitHub release at build time to link each installer
  (`.dmg`, `.exe`, `.deb`). If GitHub is unreachable, links fall back to the latest release page.

The theme follows the system setting, and the toggle in the nav saves a manual choice.
The hero download button detects the visitor's OS and links straight to its installer.

## Deploy (Cloudflare Pages)

`.github/workflows/website.yml` builds the site and deploys it with Wrangler:

- push to `main` touching `website/` → production
- a published GitHub release → production (refreshes download links)
- pull request → preview URL

One-time setup:

1. In Cloudflare, create a Pages project named `plume-website`
   (Workers & Pages → Create → Pages → Direct upload), or run
   `npx wrangler pages project create plume-website --production-branch=main` once.
2. Create an API token with the **Cloudflare Pages: Edit** permission.
3. In GitHub → Settings → Secrets and variables → Actions, add
   `CLOUDFLARE_API_TOKEN` and `CLOUDFLARE_ACCOUNT_ID`.
4. In the Pages project → Custom domains, add `speakplume.com` (and `www.speakplume.com`).
   With the domain registered at Cloudflare, DNS records are created for you.

Without the secrets, the workflow still builds the site but skips the deploy.
