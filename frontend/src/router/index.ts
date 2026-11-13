import { createRouter, createWebHistory } from 'vue-router'
import AppLayout from '@/components/AppLayout.vue'
import { useSessionStore } from '@/stores/session'

export const router = createRouter({
  history: createWebHistory(),
  routes: [
    { path: '/login', name: 'login', component: () => import('@/views/LoginView.vue'), meta: { public: true } },
    {
      path: '/',
      component: AppLayout,
      children: [
        { path: '', redirect: { name: 'users' } },
        { path: 'users', name: 'users', component: () => import('@/views/UsersView.vue') },
        { path: 'groups', name: 'groups', component: () => import('@/views/GroupsView.vue') },
        { path: 'templates', name: 'templates', component: () => import('@/views/TemplatesView.vue') },
      ],
    },
    { path: '/:pathMatch(.*)*', redirect: '/' },
  ],
})

router.beforeEach(async (to) => {
  if (to.meta.public) return true
  const session = useSessionStore()
  if (await session.restore()) return true
  return { name: 'login', query: to.fullPath === '/' ? {} : { redirect: to.fullPath } }
})
