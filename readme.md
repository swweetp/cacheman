# Cacheman

Cacheman is a tool for sharing pacman cache across hosts. It acts as an HTTP proxy server and automatically retrieves caches from other hosts.

## Usage
1. Install Cacheman on your system.
 (TODO: write install instructions)

1. Start Cacheman with the following command:
```bash 
systemctl enable --now cacheman
```

1. Prepend the following entry to the top of your `/etc/pacman.d/mirrorlist` file:
```bash
Server = http://localhost:1052/proxy/$arch/$repo
```

1. Run `pacman -Syu` to update your system. Cacheman will automatically retrieve the cache from other hosts if it is not available locally.
