# Run from maqi: vhs tapes/demo.tape
Output tapes/demo.gif

Set Shell "bash"
Set Width 1200
Set Height 460
Set FontSize 22
Set FontFamily "Menlo"
Set Theme "Catppuccin Mocha"
Set Padding 28
Set Framerate 25
Set TypingSpeed 65ms
Set CursorBlink false

Env PS1 ""
Env TERM_PROGRAM "vhs"
Env HISTFILE "/dev/null"

# 1. Insert in the middle, submit, recall history, and restore a draft.
Hide
Type@1ms "sh tapes/scene.sh 1"
Enter
Sleep 1s
Show
Sleep 2s
Type "echo hello"
Sleep 500ms
Ctrl+A
Right@100ms 5
Type "maqi: "
Ctrl+E
Type "!"
Sleep 1s
Enter
Sleep 1200ms
Type "echo draft"
Sleep 700ms
Up
Sleep 1200ms
Down
Sleep 1200ms
Ctrl+C
Sleep 500ms

# 2. Complete commands, inherited flags, choices, and nested commands.
Hide
Ctrl+D
Sleep 300ms
Type@1ms "sh tapes/scene.sh 2"
Enter
Sleep 1s
Show
Sleep 2s
Type "ku"
Tab
Sleep 700ms
Type "ge"
Tab
Sleep 700ms
Type "pods --names"
Tab
Sleep 700ms
Type "default -o "
Tab
Sleep 2s
Escape
Type "ya"
Tab
Sleep 1500ms
Enter
Sleep 1200ms
Type "kubectl config get-"
Tab
Sleep 2s
Down
Sleep 1s
Enter
Sleep 1500ms
Ctrl+C

# 3. Continue an unfinished pipeline, edit its first line, then submit.
Hide
Ctrl+D
Sleep 300ms
Type@1ms "sh tapes/scene.sh 3"
Enter
Sleep 1s
Show
Sleep 2s
Type "echo hello |"
Enter
Sleep 1s
Type "cat"
Sleep 1s
Up
Ctrl+A
Right@80ms 5
Type "maqi: "
Sleep 1s
Down
Ctrl+E
Sleep 700ms
Enter
Sleep 2500ms

Hide
Ctrl+D
