# Isolated subset of the user's Zim completion settings, using real kubectl completion.
PROMPT='probe> '
RPROMPT=''
HISTFILE=/dev/null
unsetopt BEEP
setopt AUTO_LIST AUTO_MENU ALWAYS_TO_END NO_LIST_BEEP
autoload -Uz compinit
compinit -D -i
zstyle ':completion:*:*:*:*:*' menu select
zstyle ':completion:*:matches' group yes
zstyle ':completion:*:options' description yes
zstyle ':completion:*:options' auto-description '%d'
zstyle ':completion:*:descriptions' format '%F{yellow}-- %d --%f'
zstyle ':completion:*' format '%F{yellow}-- %d --%f'
zstyle ':completion:*' group-name ''
zstyle ':completion:*' verbose yes
zstyle ':completion:*' matcher-list 'm:{a-zA-Z}={A-Za-z}' '+r:|?=**'
source <(kubectl completion zsh)
