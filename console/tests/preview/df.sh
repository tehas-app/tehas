# The df of the preview tests, the body test/preview_lib.sh gives `t_stub df`:
# the free kilobytes in $T/fx/free, 40 GB when there is none.

[ ! -e "$T/fx/df-fails" ] || exit 1
echo "Filesystem 1024-blocks Used Available Capacity Mounted on"
echo "/dev/x 60000000 1 $(cat "$T/fx/free" 2>/dev/null || echo 41943040) 1% /"
