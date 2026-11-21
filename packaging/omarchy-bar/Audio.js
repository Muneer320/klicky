.pragma library

function bytes(text) {
  return encodeURIComponent(text).replace(/%[0-9a-f]{2}|./gi, "x").length
}

function validId(id) {
  return typeof id === "string" && id.length > 0 && bytes(id) <= 1024
      && !/[\u0000-\u001f\u007f-\u009f]/.test(id)
}

function validDevice(device) {
  return device && device.backend === "pulse_audio" && validId(device.id)
      && (device.name === null || typeof device.name === "string" && device.name.length > 0 && bytes(device.name) <= 2048)
}

function validPreference(preference) {
  if (!preference) return false
  if (preference.mode === "system_default") return preference.backend === undefined && preference.id === undefined
  return preference.mode === "device" && preference.backend === "pulse_audio" && validId(preference.id)
}

function status(text) {
  if (bytes(text) > 32768) throw new Error("Audio status is too large")
  const value = JSON.parse(text)
  if (value.schema_version !== 1 || ["initializing", "available", "unavailable", "recovering", "inactive"].indexOf(value.availability) < 0
      || typeof value.manual_paused !== "boolean" || typeof value.can_select !== "boolean"
      || typeof value.can_enumerate !== "boolean" || typeof value.can_retry !== "boolean"
      || !(value.requested === null || validPreference(value.requested))
      || !(value.active === null || validDevice(value.active))) throw new Error("Unsupported audio status")
  if (value.active && (!value.requested || value.availability !== "available"
      || value.requested.mode === "device" && value.requested.id !== value.active.id)) throw new Error("Mismatched audio output")
  return value
}

function outputs(text) {
  if (bytes(text) > 32768) throw new Error("Audio list is too large")
  const value = JSON.parse(text)
  if (value.schema_version !== 1 || !Array.isArray(value.devices) || value.devices.length > 64
      || !value.devices.every(validDevice)) throw new Error("Unsupported audio outputs")
  const ids = value.devices.map(function(device) { return device.id })
  if (ids.some(function(id, index) { return ids.indexOf(id) !== index })
      || [value.default_id, value.suggested_speakers_id].some(function(id) { return id !== null && ids.indexOf(id) < 0 })) throw new Error("Invalid audio identity")
  return value
}

function selection(state) {
  if (!state || !state.requested) return ""
  return state.requested.mode === "system_default" ? "system_default" : "device:" + state.requested.id
}

function options(state, list, defaultLabel) {
  if (!state || !state.requested) return []
  const choices = [{value: "system_default", label: defaultLabel}]
  const devices = list ? list.devices : []
  devices.forEach(function(device, index) {
    let label = device.name || device.id
    if (devices.filter(function(other) { return (other.name || other.id) === label }).length > 1) label += " (" + (index + 1) + ")"
    choices.push({value: "device:" + device.id, label: label})
  })
  return choices
}

function command(value, state, list) {
  if (!state || !state.requested || !state.can_select) return null
  if (value === "system_default") return ["audio", "follow-default"]
  if (typeof value !== "string" || value.indexOf("device:") !== 0 || !list) return null
  const id = value.slice(7)
  if (!list.devices.some(function(device) { return device.id === id })) return null
  return ["audio", "select", "--backend", "pulse-audio", "--id", id]
}
