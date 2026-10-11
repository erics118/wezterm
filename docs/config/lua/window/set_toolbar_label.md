# `window:set_toolbar_label{...}`

{{since('nightly')}}

Sets the label before the tabs of the native titlebar, shown when
[macos_native_tab_bar](../config/macos_native_tab_bar.md) is `true`.
Clicking the label pops up its menu, and choosing an entry emits
[toolbar-menu-item](../window-events/toolbar-menu-item.md) with the entry's
`id`.

The table takes:

* `symbol` - an SF Symbols name shown before the text, or none when empty
* `symbol_color` - the symbol's color, or the secondary label color when unset
* `title` - the bold line
* `subtitle` - the line under it
* `dimmed` - when `true`, both lines take dimmer colors
* `menu` - a list of entries, each with `id`, `title`, an optional
  `checked` flag and an optional `icon`, a table of an SF Symbols `symbol`
  and its `color`. An entry with an empty `title` is a separator.

The label keeps one width, so a long title fades out at its end.

```lua
wezterm.on('update-status', function(window, pane)
  window:set_toolbar_label {
    symbol = 'laptopcomputer',
    title = wezterm.hostname(),
    subtitle = 'This Mac',
    menu = {
      { id = 'new', title = 'New Tab' },
    },
  }
end)
```
