# Fixed program invoked through pkexec; subnet and port are validated by Rust
# and passed as positional arguments, never interpolated into shell source.
set -eu
PATH=/usr/sbin:/usr/bin:/sbin:/bin
export PATH
subnet=$1
port=$2
ufw allow from "$subnet" to any port "$port" proto tcp comment 'on-air LAN audio'
ufw allow from "$subnet" to any port 48000:48127 proto udp comment 'on-air AirPlay timing/control'
# UFW can be marked disabled while its old kernel chains remain active.
# Its CLI saves the rule but skips the live update in that state.
if iptables -w 5 -S ufw-user-input >/dev/null 2>&1; then
    if ! iptables -w 5 -C ufw-user-input -s "$subnet" -p tcp --dport "$port" -j ACCEPT; then
        iptables -w 5 -I ufw-user-input 1 -s "$subnet" -p tcp --dport "$port" -j ACCEPT
    fi
    if ! iptables -w 5 -C ufw-user-input -s "$subnet" -p udp --dport 48000:48127 -j ACCEPT; then
        iptables -w 5 -I ufw-user-input 1 -s "$subnet" -p udp --dport 48000:48127 -j ACCEPT
    fi
fi
