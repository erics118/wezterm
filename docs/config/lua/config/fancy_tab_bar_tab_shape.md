---
tags:
  - appearance
  - tab_bar
---

# `fancy_tab_bar_tab_shape = "Tab"`

{{since('nightly')}}

Configures the shape of each tab when the fancy tab bar is in use.

Possible values are:

* `"Tab"` - the tab rounds the two corners away from the terminal and joins
  its edge
* `"Pill"` - the tab rounds all four corners and is centered vertically in
  the tab bar, with a gap to its neighbours

```lua
config.fancy_tab_bar_tab_shape = 'Pill'
```
