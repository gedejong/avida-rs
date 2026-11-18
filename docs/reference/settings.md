# Settings reference

Since 2026-11-18 the settings are part of the experiment file, `avida.yaml`, where they are grouped (`world.x`, `mutation.copy_prob`). Every setting is listed with its group, its key, its name in the old `avida.cfg`, its type, its default and its description in [the experiment file reference](config-yaml.md#settings-by-group), which is written from the program's own table. The same page lists the [old names](config-yaml.md#old-names) in alphabetical order with the place each one went.

On the command line a setting is changed with its dotted path: `avida -set world.x 30 -set world.y 30`. `avida -r` prints a review of the settings that differ from their defaults, and `avida -w` warns about each default that gets used.
