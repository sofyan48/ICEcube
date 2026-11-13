<script setup lang="ts">
import { SwitchRoot, SwitchThumb } from 'reka-ui'
import { Sparkles } from 'lucide-vue-next'
import { computed, ref, watch } from 'vue'
import { errorMessage } from '@/lib/api'
import { NAME_RE, sameBindings } from '@/lib/rbac'
import type { Binding, Group } from '@/lib/types'
import { useGroupsStore } from '@/stores/groups'
import { useToastStore } from '@/stores/toast'
import BindingEditor from './BindingEditor.vue'
import UiButton from './ui/UiButton.vue'
import UiDialog from './ui/UiDialog.vue'

const props = defineProps<{ group?: Group | null }>()
const open = defineModel<boolean>('open', { required: true })

const groups = useGroupsStore()
const toast = useToastStore()

const name = ref('')
const description = ref('')
const bindings = ref<Binding[]>([])
const isDefault = ref(false)
const saving = ref(false)
const editing = computed(() => !!props.group)
const memberCount = computed(() => (props.group ? groups.members(props.group.name).length : 0))

watch(open, (isOpen) => {
  if (!isOpen) return
  name.value = props.group?.name ?? ''
  description.value = props.group?.description ?? ''
  bindings.value = [...(props.group?.bindings ?? [])]
  isDefault.value = props.group?.defaultForNewUsers ?? false
})

const nameError = computed(() => {
  if (editing.value || !name.value) return null
  if (!NAME_RE.test(name.value)) return 'Lowercase letters, digits and "-" only.'
  if (groups.byName.has(name.value)) return 'A group with this name already exists.'
  return null
})
const dirty = computed(
  () =>
    !editing.value ||
    (props.group?.description ?? '') !== description.value ||
    props.group?.defaultForNewUsers !== isDefault.value ||
    !sameBindings(props.group?.bindings ?? [], bindings.value),
)

async function save() {
  saving.value = true
  const input = { name: name.value, description: description.value, bindings: bindings.value, defaultForNewUsers: isDefault.value }
  try {
    if (editing.value) await groups.update(input)
    else await groups.create(input)
    toast.success(
      editing.value ? 'Group updated' : 'Group created',
      editing.value && memberCount.value ? `Access re-applied for ${memberCount.value} member(s).` : name.value,
    )
    open.value = false
  } catch (e) {
    toast.error('Could not save group', errorMessage(e))
  } finally {
    saving.value = false
  }
}
</script>

<template>
  <UiDialog
    v-model:open="open"
    width="xl"
    :title="editing ? `Edit group ${group?.name}` : 'New group'"
    description="Members get every grant of the group. Changes apply to all members right away."
  >
    <div class="grid gap-4 sm:grid-cols-2">
      <label class="block">
        <span class="mb-1.5 block text-xs font-medium text-zinc-600 dark:text-zinc-400">Name</span>
        <input v-model.trim="name" :disabled="editing" class="field font-mono" placeholder="backend-team" autocomplete="off" />
        <span v-if="nameError" class="mt-1 block text-xs text-red-600">{{ nameError }}</span>
      </label>
      <label class="block">
        <span class="mb-1.5 block text-xs font-medium text-zinc-600 dark:text-zinc-400">Description</span>
        <input v-model="description" class="field" placeholder="What members of this group can do" />
      </label>
    </div>

    <label class="mt-4 flex cursor-pointer items-center justify-between gap-4 rounded-xl border border-zinc-200 p-3 dark:border-zinc-800">
      <span class="flex items-start gap-3">
        <Sparkles class="mt-0.5 size-4 text-emerald-500" />
        <span>
          <span class="block text-sm font-medium">Default for new users</span>
          <span class="block text-xs text-zinc-500">Preselected when creating a user. Existing members are not affected.</span>
        </span>
      </span>
      <SwitchRoot
        v-model="isDefault"
        class="relative flex h-6 w-10 shrink-0 rounded-full bg-zinc-200 p-0.5 transition data-[state=checked]:bg-brand-600 dark:bg-zinc-700"
      >
        <SwitchThumb class="size-5 rounded-full bg-white shadow transition data-[state=checked]:translate-x-4" />
      </SwitchRoot>
    </label>

    <div class="mt-6"><BindingEditor v-model="bindings" /></div>

    <template #footer>
      <span v-if="memberCount" class="mr-auto text-xs text-zinc-500">{{ memberCount }} member(s) will be updated</span>
      <UiButton variant="secondary" @click="open = false">Cancel</UiButton>
      <UiButton :disabled="!name || !!nameError || !dirty" :loading="saving" @click="save">
        {{ editing ? 'Save changes' : 'Create group' }}
      </UiButton>
    </template>
  </UiDialog>
</template>
