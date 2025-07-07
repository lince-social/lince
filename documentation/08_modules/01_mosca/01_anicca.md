# Anicca

# Warning
This is a foul piece of software, dangerous and destructive on unmastered hands.

# Install
Download the repo, then run:

```bash
# Sudo means to run in Super User DO mode, where you have 'admin' capabilities,
# so the program can make actions that require elevated privileges.
# This is necessary for manipulating files that the system 'relies on' like '.cache';
# which in my machine is harmless to delete but without the 'sudo -E...' doesn't work.
sudo -E cargo run
```

Or download the latest Release's binary and then run:

```bash
./os
```

# Configuration
The path to the configuration file is in your config dir + lince/os.toml

In Linux (Arch (with Archinstall) btw), the only system I tested it, the path would be ~/.config/lince/os.toml.

Anicca means 'impermanence' in Pali, Nicca means the contrary: permanence. A fitting name for the specification of what persists. The Anicca feature deletes all files and directories that are in the same dir as a specified file or directory. Nicca specifies files and dirs that should be ignored from that clean slate wipe.

This was robbed from NixOS's [Impermanence](https://github.com/nix-community/impermanence) flake, the only reason I stayed in NixOS for so long. I wanted that feature in every distro and built this.

If I where to summarize to the atomic level what this feature does when it reads your dotfile at os.toml it would be this:

> One dir up that line is put into a 'list' of dirs that will have everything in it removed, except the dir specified.

(What?)

Let's look into some examples.

Let's say that the name of my user is 'myname', in this case the Home Directory would be '/home/myname', lets check out our mock dirs:

```linuxhome
/home/myname/.config/lince
/home/myname/.config/helix
/home/myname/.config/nvim
/home/myname/.cache
/home/myname/Downloads
```

If LinceOS's config file's contents are:

```toml
[nicca]
list = [
 "/home/myname/.config"
]
```

Every dir inside 'home/myname', except for .config, it's subdirectories and files will be deleted. The remaining ones will be these:

```linuxhome
/home/myname/.config/lince
/home/myname/.config/helix
/home/myname/.config/nvim
```
---

Great! Now let's say that LinceOS's config is this:

```toml
[nicca]
list = [
 "/home/myname/.config/lince",
 "/home/myname/.config/helix",
 "/home/myname/Downloads"
]
```

The remaining dirs and files will be these:

```linuxhome
/home/myname/.config/lince
/home/myname/.config/helix
/home/myname/Downloads
```

The deleted ones:

```linuxhome
/home/myname/.config/nvim
/home/myname/.cache
```

When we add one level of 'immersion' inside subdirs we make every 'sibling' subdir also elegible for removing. In the first example with just 'home/myname/.config' the '.config/nvim' dir wasn't specified, therefore it wasn't deleted, all in .config where spared.

But in the second case when specific subdirs in '.config' like 'helix', the program thought that dirs (and files) inside .config (except for the specified ones: 'lince' and 'helix') should be deleted. This removed nvim because this hypothetic user knows that it is objectivelly worse than helix.

Since we didn't specify anything in the root ('/') dir, LinceOS will not touch it, it only goes one dir up on each line in the config file, so if some user specifies in it's config file:
```toml
[nicca]
list = [
 "/home"
]
```
Every other dir in '/' will be deleted, that's bad in most cases, if you don't know if it's bad for you, it's bad for you.
