import { defineConfig } from 'astro/config';

export default defineConfig({
  site: 'https://speakplume.com',
  trailingSlash: 'ignore',
  i18n: {
    defaultLocale: 'en',
    locales: ['en', 'fr'],
    routing: { prefixDefaultLocale: false },
  },
});
