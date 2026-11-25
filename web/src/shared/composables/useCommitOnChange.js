import { ref, watch } from 'vue'

// For text inputs: let the user type freely against a local draft, and only
// commit the trimmed value (on blur / enter). If the stored value changes from
// elsewhere, the draft follows it.
export function useCommitOnChange(source, commit) {
  const draft = ref(source())

  watch(source, (value) => {
    if (value !== draft.value) draft.value = value
  })

  const onChange = () => commit(draft.value.trim())

  return { draft, onChange }
}
