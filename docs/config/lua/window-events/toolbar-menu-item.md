# `toolbar-menu-item`

{{since('nightly')}}

The `toolbar-menu-item` event is emitted when an entry of the menu set by
[window:set_toolbar_label](../window/set_toolbar_label.md) is chosen. The
handler gets the window, the active pane and the entry's `id`.

```lua
wezterm.on('toolbar-menu-item', function(window, pane, id)
  if id == 'new' then
    window:perform_action(wezterm.action.SpawnTab 'CurrentPaneDomain', pane)
  end
end)
```
