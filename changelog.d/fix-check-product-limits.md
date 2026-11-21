### Changes

- `tehas check-product --sandbox` runs the product's install and gates with the memory and CPU limits a cycle's container gets on the host, in place of a fixed 4 GiB. A product whose build needs more than 4 GiB on a host with many CPUs no longer fails the check with its compiler killed. When the slice cannot be used, the check stops with 78, as `tehas work` does.
