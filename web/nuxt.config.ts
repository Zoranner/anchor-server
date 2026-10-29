import tailwindcss from '@tailwindcss/vite'

export default defineNuxtConfig({
  srcDir: 'app/',
  ssr: false,
  compatibilityDate: '2026-10-30',
  modules: ['@nuxt/icon', '@stellar/ui/nuxt'],
  icon: {
    provider: 'none',
    componentName: 'NuxtIcon',
    clientBundle: {
      scan: {
        globInclude: ['**/*.{vue,jsx,tsx,ts,md,mdc,mdx,yml,yaml}'],
      },
    },
  },
  css: ['~/assets/css/main.css'],
  devServer: {
    port: 18082,
  },
  vite: {
    plugins: [tailwindcss()],
  },
  nitro: {
    preset: 'static',
    devProxy: {
      '/api': { target: 'http://127.0.0.1:8080', changeOrigin: true },
      '/healthz': { target: 'http://127.0.0.1:8080', changeOrigin: true },
      '/readyz': { target: 'http://127.0.0.1:8080', changeOrigin: true },
    },
  },
  app: {
    head: {
      title: 'Anchor Guard 管理台',
      meta: [
        {
          name: 'viewport',
          content: 'width=device-width, initial-scale=1',
        },
      ],
    },
  },
  typescript: {
    strict: true,
  },
  devtools: {
    enabled: false,
  },
})
