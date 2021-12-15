function ThrowOnNativeFailure {
    if (-not $?)
    {
        throw 'Native Failure'
    }
}

wsl cargo build --release --package=wurstminebot-cli --package=wurstminebot-python
ThrowOnNativeFailure

ssh wurstmineberg.de sudo systemctl stop wurstminebot
ThrowOnNativeFailure

scp .\target\release\wurstminebot wurstmineberg@wurstmineberg.de:/opt/wurstmineberg/bin/wurstminebot
ThrowOnNativeFailure

scp .\target\release\libwurstminebot.so wurstmineberg@wurstmineberg.de:/opt/py/wurstminebot.so
ThrowOnNativeFailure

ssh wurstmineberg.de sudo systemctl start wurstminebot
ThrowOnNativeFailure

ssh wurstmineberg.de sudo systemctl reload uwsgi
ThrowOnNativeFailure
