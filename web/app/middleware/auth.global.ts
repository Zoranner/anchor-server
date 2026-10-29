export default defineNuxtRouteMiddleware(async (to) => {
  const api = useAdminApi()
  const active = await api.restoreSession()
  if (to.path === '/login') {
    return active ? navigateTo('/devices') : undefined
  }
  return active ? undefined : navigateTo('/login')
})
