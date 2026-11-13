import { defineStore } from 'pinia'
import { computed, ref } from 'vue'
import { api, clearCredentials, handleUnauthorized, setCredentials } from '@/lib/api'
import type { Session } from '@/lib/types'

/**
 * Credentials live only in memory (and sessionStorage, so a reload in the same
 * tab keeps you signed in). Closing the tab signs you out.
 */
const STORAGE_KEY = 'icecube.auth'

export const useSessionStore = defineStore('session', () => {
  const session = ref<Session | null>(null)
  const isAuthenticated = computed(() => session.value !== null)

  async function login(user: string, password: string) {
    setCredentials(user, password)
    try {
      session.value = await api.get<Session>('/session')
      sessionStorage.setItem(STORAGE_KEY, JSON.stringify({ user, password }))
    } catch (e) {
      clearCredentials()
      throw e
    }
  }

  async function restore(): Promise<boolean> {
    if (session.value) return true
    const raw = sessionStorage.getItem(STORAGE_KEY)
    if (!raw) return false
    try {
      const { user, password } = JSON.parse(raw)
      await login(user, password)
      return true
    } catch {
      sessionStorage.removeItem(STORAGE_KEY)
      return false
    }
  }

  function logout() {
    session.value = null
    clearCredentials()
    sessionStorage.removeItem(STORAGE_KEY)
  }

  handleUnauthorized(logout)

  return { session, isAuthenticated, login, restore, logout }
})
