---
tags:
  - appearance
  - tab_bar
---

# `fancy_tab_bar_alignment = "Left"`

{{since('nightly')}}

Configures where the tabs sit when the fancy tab bar is in use.

Possible values are:

* `"Left"` - the tabs start at the left of the tab bar
* `"Center"` - the tabs are centered in the window, but never overlap the
  space reserved on the left for integrated window buttons

```lua
config.fancy_tab_bar_alignment = 'Center'
```
