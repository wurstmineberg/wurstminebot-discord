#!/usr/bin/env pwsh

cargo check --package=wurstminebot-cli --package=wurstminebot-python
if (-not $?)
{
    throw 'Native Failure'
}

cargo +beta check --package=wurstminebot-cli --package=wurstminebot-python
if (-not $?)
{
    throw 'Native Failure'
}

# copy the tree to the WSL file system to improve compile times
wsl rsync --delete -av /mnt/c/Users/fenhl/git/github.com/wurstmineberg/wurstminebot-discord/stage/ /home/fenhl/wslgit/github.com/wurstmineberg/wurstminebot-discord/ --exclude target
if (-not $?)
{
    throw 'Native Failure'
}

wsl env -C /home/fenhl/wslgit/github.com/wurstmineberg/wurstminebot-discord cargo check --package=wurstminebot-cli --package=wurstminebot-python
if (-not $?)
{
    throw 'Native Failure'
}
