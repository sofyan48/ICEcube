<script setup lang="ts">
import { TabsContent, TabsIndicator, TabsList, TabsRoot, TabsTrigger } from 'reka-ui'
import { computed, ref, watch } from 'vue'
import { ApiError, errorMessage } from '@/lib/api'
import { bindingObject, NAME_RE, sameBindings } from '@/lib/rbac'
import type { Binding, CredentialKind, User } from '@/lib/types'
import { toYaml } from '@/lib/yaml'
import { useGroupsStore } from '@/stores/groups'
import { useToastStore } from '@/stores/toast'
import { useUsersStore } from '@/stores/users'
import BindingEditor from './BindingEditor.vue'
import EffectiveAccess from './EffectiveAccess.vue'
import GroupPicker from './GroupPicker.vue'
import UiButton from './ui/UiButton.vue'
import UiCode from './ui/UiCode.vue'
import UiDialog from './ui/UiDialog.vue'

const props = defineProps<{ user?: User | null }>()
const open = defineModel<boolean>('open', { required: true })
const emit = defineEmits<{ saved: [User] }>()

const users = useUsersStore()
const groups = useGroupsStore()
const toast = useToastStore()

const name = ref('')
const description = ref('')
const bindings = ref<Binding[]>([])
const memberOf = ref<string[]>([])
const credential = ref<CredentialKind>('certificate')
/** Server message when the name belonged to a recently deleted user. */
const reuseBlocked = ref<string | null>(null)
const allowNameReuse = ref(false)
const tab = ref('groups')
const saving = ref(false)

const editing = computed(() => !!props.user)

watch(open, (isOpen) => {
  if (!isOpen) return
  name.value = props.user?.name ?? ''
  description.value = props.user?.description ?? ''
  bindings.value = [...(props.user?.bindings ?? [])]
  // New users start in the default groups, so they work without extra setup.
  memberOf.value = props.user ? [...props.user.groups] : [...groups.defaults]
  credential.value = props.user?.credential ?? 'certificate'
  reuseBlocked.value = null
  allowNameReuse.value = false
  tab.value = 'groups'
})

watch(name, () => {
  reuseBlocked.value = null
  allowNameReuse.value = false
})

const credentialOptions: { value: CredentialKind; label: string; hint: string }[] = [
  { value: 'certificate', label: 'Client certificate', hint: 'For people. Cannot be revoked; works until it expires.' },
  { value: 'serviceAccount', label: 'ServiceAccount token', hint: 'For CI/CD or anyone who must be revocable at once.' },
]

const nameError = computed(() => {
  if (editing.value || !name.value) return null
  if (!NAME_RE.test(name.value)) return 'Lowercase letters, digits and "-" only.'
  if (users.byName.has(name.value)) return 'A user with this name already exists.'
  return null
})

const sameGroups = (a: string[], b: string[]) => a.length === b.length && a.every((g) => b.includes(g))
const dirty = computed(
  () =>
    !editing.value ||
    (props.user?.description ?? '') !== description.value ||
    !sameBindings(props.user?.bindings ?? [], bindings.value) ||
    !sameGroups(props.user?.groups ?? [], memberOf.value) ||
    (props.user?.credential ?? 'certificate') !== credential.value,
)
const canSave = computed(
  () => !!name.value && !nameError.value && dirty.value && (!reuseBlocked.value || allowNameReuse.value || credential.value === 'serviceAccount'),
)

/** Computed locally, exactly like the backend: direct grants + group grants. */
const effective = computed(() => groups.effective({ bindings: bindings.value, groups: memberOf.value }))
const preview = computed(() => toYaml(effective.value.map((b) => bindingObject(name.value || '<name>', b))))

async function save() {
  saving.value = true
  const input = {
    name: name.value,
    description: description.value,
    bindings: bindings.value,
    groups: memberOf.value,
    credential: credential.value,
    allowNameReuse: allowNameReuse.value,
  }
  try {
    const saved = editing.value ? await users.update(input) : await users.create(input)
    toast.success(editing.value ? 'User updated' : 'User created', `${effective.value.length} binding(s) applied.`)
    open.value = false
    emit('saved', saved)
  } catch (e) {
    if (!editing.value && e instanceof ApiError && e.status === 409 && e.message.includes('deleted recently')) {
      reuseBlocked.value = e.message
      return
    }
    toast.error('Could not save user', errorMessage(e))
  } finally {
    saving.value = false
  }
}

const tabClass =
  'relative z-10 flex items-center gap-1.5 px-3 py-1.5 text-sm font-medium text-zinc-500 transition data-[state=active]:text-zinc-900 dark:data-[state=active]:text-white'
const countClass = 'rounded-full bg-zinc-200/80 px-1.5 text-[10px] tabular-nums dark:bg-zinc-700'
</script>

<template>
  <UiDialog
    v-model:open="open"
    width="xl"
    :title="editing ? `Edit ${user?.name}` : 'New user'"
    :description="editing ? 'Change what this user can access.' : 'Default groups are preselected, so the user is ready to go.'"
  >
    <div class="grid gap-4 sm:grid-cols-2">
      <label class="block">
        <span class="mb-1.5 block text-xs font-medium text-zinc-600 dark:text-zinc-400">Username</span>
        <input v-model.trim="name" :disabled="editing" class="field font-mono" placeholder="jane-doe" autocomplete="off" autofocus />
        <span v-if="nameError" class="mt-1 block text-xs text-red-600">{{ nameError }}</span>
      </label>
      <label class="block">
        <span class="mb-1.5 block text-xs font-medium text-zinc-600 dark:text-zinc-400">Description</span>
        <input v-model="description" class="field" placeholder="Backend team" />
      </label>
    </div>

    <fieldset class="mt-4">
      <legend class="mb-1.5 block text-xs font-medium text-zinc-600 dark:text-zinc-400">Credential</legend>
      <div class="grid gap-2 sm:grid-cols-2">
        <label
          v-for="o in credentialOptions"
          :key="o.value"
          class="flex cursor-pointer gap-2.5 rounded-xl border p-3 text-sm transition"
          :class="
            credential === o.value
              ? 'border-brand-500 bg-brand-50/50 dark:bg-brand-500/10'
              : 'border-zinc-200 hover:border-zinc-300 dark:border-zinc-800 dark:hover:border-zinc-700'
          "
        >
          <input v-model="credential" type="radio" :value="o.value" class="mt-0.5 accent-brand-600" />
          <span>
            <span class="block font-medium">{{ o.label }}</span>
            <span class="block text-xs text-zinc-500">{{ o.hint }}</span>
          </span>
        </label>
      </div>
      <p
        v-if="editing && user?.credential === 'certificate' && credential === 'serviceAccount'"
        class="mt-2 text-xs text-amber-700 dark:text-amber-300"
      >
        Bindings move to the ServiceAccount: certificates issued so far lose all access. Issue a new kubeconfig afterwards.
      </p>
      <p
        v-else-if="editing && user?.credential === 'serviceAccount' && credential === 'certificate'"
        class="mt-2 text-xs text-amber-700 dark:text-amber-300"
      >
        The ServiceAccount is deleted, so its tokens stop working. Issue a new kubeconfig afterwards.
      </p>
    </fieldset>

    <div v-if="reuseBlocked" class="mt-4 rounded-xl bg-red-50 p-3 text-xs text-red-800 dark:bg-red-500/10 dark:text-red-200">
      <p>{{ reuseBlocked }}</p>
      <p v-if="credential === 'serviceAccount'" class="mt-2">
        ServiceAccount users are not affected by old certificates, so this user can be created.
      </p>
      <label v-else class="mt-2 flex items-center gap-2 font-medium">
        <input v-model="allowNameReuse" type="checkbox" class="accent-red-600" />
        Reuse the name anyway; the old certificates will work again
      </label>
    </div>

    <TabsRoot v-model="tab" class="mt-6">
      <TabsList class="relative mb-4 inline-flex rounded-lg bg-zinc-100 p-1 dark:bg-zinc-800/70">
        <TabsIndicator
          class="absolute top-1 left-0 h-[calc(100%-0.5rem)] w-(--reka-tabs-indicator-size) translate-x-(--reka-tabs-indicator-position) rounded-md bg-white shadow-sm transition-all duration-200 dark:bg-zinc-700"
        />
        <TabsTrigger value="groups" :class="tabClass">Groups <span :class="countClass">{{ memberOf.length }}</span></TabsTrigger>
        <TabsTrigger value="direct" :class="tabClass">Direct access <span :class="countClass">{{ bindings.length }}</span></TabsTrigger>
        <TabsTrigger value="effective" :class="tabClass">Effective <span :class="countClass">{{ effective.length }}</span></TabsTrigger>
        <TabsTrigger value="yaml" :class="tabClass">YAML</TabsTrigger>
      </TabsList>
      <TabsContent value="groups"><GroupPicker v-model="memberOf" /></TabsContent>
      <TabsContent value="direct">
        <p class="mb-3 text-xs text-zinc-500">Extra grants on top of what the groups give.</p>
        <BindingEditor v-model="bindings" />
      </TabsContent>
      <TabsContent value="effective"><EffectiveAccess :bindings="effective" /></TabsContent>
      <TabsContent value="yaml">
        <UiCode v-if="effective.length" :code="preview" max-height="28rem" />
        <p v-else class="py-8 text-center text-sm text-zinc-500">No bindings will be created.</p>
      </TabsContent>
    </TabsRoot>

    <template #footer>
      <span class="mr-auto text-xs text-zinc-500">{{ effective.length }} effective binding(s)</span>
      <UiButton variant="secondary" @click="open = false">Cancel</UiButton>
      <UiButton :disabled="!canSave" :loading="saving" @click="save">
        {{ editing ? 'Save changes' : 'Create user' }}
      </UiButton>
    </template>
  </UiDialog>
</template>
