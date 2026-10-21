-- on-air hotkey for Omarchy 4. Append to ~/.config/hypr/bindings.lua.
-- Same slot family as Omarchy's music TUI (SUPER + SHIFT + ALT + M);
-- `focus = true` raises the running window instead of opening a second.
o.bind("SUPER + SHIFT + ALT + O", "On Air", { tui = "on-air", focus = true })
