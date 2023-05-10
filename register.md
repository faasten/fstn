```sh
# install fstn on the branch `yue`
git clone git@github.com:faasten/fstn
cd fstn
cargo install --path .
export FSTN_SERVER=sns59.cs.princeton.edu
# faasten relies on authentication to create you your private `fsutil` gate (the gate path is `home:<login,login>:fsutil`)
fstn login
# `fstn register` to see positional params.
# faasten expands tilde `~` to `home:<login,login>`.
fstn register ./output/thumbnail.img 'yuetan,yuetan' '~:thumbnail.img' 128 python
```
