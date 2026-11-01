---
title: Resize or restart the host
description: "Changing the size of a tehas host's virtual machine, and restarting what it runs on, without a drain: the headroom to set once, the live commands, and when a drain is still the answer."
---

# Resize or restart the host

A factory is one machine, and on the hosts we run it that machine is a virtual
one, so the two operations that change it, giving it more CPUs or memory and
restarting the hypervisor under it, have been done by emptying it first: `tehas
drain`, then `virsh shutdown` on the hypervisor, then a resume, with about an
hour in which the factory had slots to fill and no cycles in them.

This page is what a person does on the hypervisor instead. Nothing in it has
been proven on a host. The factory has no resize command and its drain has no
pause, both of which the other tickets of this epic take on, and no save of a
running factory's VM has been seen. What follows is written from libvirt and
from the two facts of tehas it rests on: `tehas.slice`'s `MemoryMax` is a
percentage, and a container's limits are read as it starts. The [features
page](/features) has rows for live resize and for a hypervisor restart with
cycles running, both marked unproven. Where a command needs the guest's
agreement it needs the guest agent, `qemu-guest-agent`, running in the VM:
check it with `systemctl status qemu-guest-agent` before you start.

The one rule underneath all of it: **the maximum is the change that needs a
restart, so set it once, high, and treat every change under it as a live one.**

## Define the VM with headroom, once

With libvirt, the shape of the VM decides what can change later without a
shutdown. In the example, the tehas-tehas host of [the host page](/host): 8
CPUs and 15 GB of memory, and every change to either of them a drain.

### CPUs

`<vcpu>` holds the hot-plug maximum as its text and the number booted as
`current`. The maximum has to multiply out of the topology exactly, or libvirt
refuses the define:

```xml
<vcpu current='8'>32</vcpu>
<cpu>
  <topology sockets='2' dies='1' cores='8' threads='2'/>
</cpu>
```

The topology above multiplies out to the 32 of `<vcpu>`: 2 sockets, 8 cores
and 2 threads each.

Count the maximum with room above today: the CPUs the model backends need
alongside the factory, one for each issue the host works on side by side, and a
margin. `TEHAS_TICKET_CONCURRENCY` defaults to the host's CPUs minus one
([the slice and its limits](/host#limits-and-concurrency)), so a host whose
maximum is 32 can grow to 31 cycles' worth of CPU quota without a restart. The
slice's ceiling is memory, not CPUs, so a CPU growth needs no change in tehas.

### Memory

Give memory as a `virtio-mem` device: a block that grows and shrinks in units
while the guest runs, and the guest adds and onlines the blocks itself.

```xml
<devices>
  <memory model='virtio-mem'>
    <target>
      <size unit='GiB'>0</size>
      <requested unit='GiB'>12</requested>
      <block unit='MiB'>128</block>
      <node0>
        <size unit='GiB'>32</size>
      </node0>
    </target>
  </memory>
</devices>
```

The alternative, for a guest without `virtio-mem`, is DIMM slots under
`<maxMemory>`: `<maxMemory slots='16' unit='GiB'>32</maxMemory>`, with one
`<memory>` element per DIMM and the same total in them all. A guest of that
shape onlines a DIMM the hypervisor adds, and takes one back only at a reboot.
Where the host cannot spare the address space, ask for fewer slots of a larger
size rather than many small ones.

Name the device so the commands below have something to type: `virsh dumpxml`
answers the device's `alias` (`virtio-mem1` in the example), and `--device`
takes that name.

`tehas.slice` takes `MemoryMax=75%` of whatever the machine has
([the slice](/host#the-slice)), so memory follows itself: the next container
after a growth gets a ceiling of the new slice over `TEHAS_SLICE_CONTAINERS`,
and the dispatcher admits cycles on the room systemd reports. Nothing in
`instance.env` holds a size of the machine.

### Applying the maximum

Changing the maximum, or the topology, is a `virsh define` of the new XML and a
`virsh destroy`/`virsh start` of the VM, which is a power cycle, so it is a
drain:

```
tehas drain --wait 3600                 # start nothing, wait for the cycles
# on the hypervisor: virsh dumpxml tehas-tehas > /tmp/tehas-tehas.xml, edit it,
# virsh define /tmp/tehas-tehas.xml, virsh shutdown tehas-tehas, virsh start tehas-tehas
tehas limit clear                       # after the VM is back and the units are up
```

That is the last drain the machine needs, provided the maximum was set above
what the next year asks for. Set it with the room the machine can give, not with
today's need.

## Grow or shrink live

Under the maximum, nothing restarts. On the hypervisor, as root, for the VM
named `tehas-tehas`:

```
virsh setvcpus tehas-tehas 12 --live          # 12 vCPUs now, booted at 8
virsh update-memory-device tehas-tehas --config --requested-size 20G   # virtio-mem, live
# or, for DIMMs:
virsh setmem tehas-tehas 20G --live
```

`--config` on the virtio-mem line is what `update-memory-device` takes for the
request; it asks for the size and libvirt hot-adds the blocks. `setmaxmem`
changes only the maximum, which is a reboot's business, not a live one.

`setvcpus --live` raises or lowers what the guest runs with now; `--config`
alone changes only what the next boot starts with, and the running guest keeps
what it has. A shrink of vCPUs asks the guest to take them back and fails when
a CPU is busy, so lower the load or the cycles first, or leave the count where
it is; the quota, not the count, is what holds a container.

What the guest does by itself, with nothing typed in it:

| Change | What the guest does |
| --- | --- |
| vCPUs up | The kernel onlines the new CPU and `nproc` counts it. A CPU held by a running process stays where it is; nothing moves on purpose |
| vCPUs down | The kernel offlines the ones the guest gives back and `nproc` drops. A cycle already running keeps the CPU quota its container was given at its start, so its gate runs on the share it began with |
| virtio-mem up | The driver takes each block the hypervisor offers, onlines it, and `MemTotal` in `/proc/meminfo` grows. `systemctl show tehas.slice -p MemoryMax` then answers the new ceiling, because the slice's is a percentage of the machine |
| virtio-mem down | The driver takes back blocks the guest can spare. It refuses down to the size it cannot free, so the number asked for is not always the number you get |
| DIMMs | The guest onlines the whole DIMM the hypervisor adds; taking one back is a reboot |

Nothing in tehas has to be told about the new size. The slice's limit is a
percentage of the machine and each container's ceiling is the slice's limit
over `TEHAS_SLICE_CONTAINERS`, read when that container starts
([`console slice`](/host#the-slice)), so the cycles that start after the growth
are the ones that see it. A cycle, a planning turn or a session that is already
running keeps the limits it began with; its memory does not grow under it.

After a growth, check it from the guest and from the factory:

```
nproc                                       # the CPUs the guest sees
free                                        # MemTotal: what the guest has onlined
systemctl show tehas.slice -p MemoryMax -p MemoryCurrent   # the slice's ceiling, and what it holds
tehas status                                # the slice line: room for N cycles
```

The `slice` line of `tehas status` is what the dispatcher's last poll read,
with that poll's own age ([`tehas status`](/tools#tehas-status)), so after a
growth it can still show the old figures: the poll that updates it runs on its
own within two minutes, or now with `tehas dispatch`. Ask the slice itself
for the new ceiling with the `systemctl show` line above, which reads the slice
and not the poll's record of it.

## Restart the hypervisor without a drain

What restarts the machine under a factory is usually the hypervisor's own
kernel. `libvirt-guests` is the unit that saves each running VM to disk and
brings it back when the hypervisor boots again, and a VM saved that way comes
back where it stopped, with its processes in the same state.

### Set it up once

```
# /etc/default/libvirt-guests
ON_SHUTDOWN=suspend
SHUTDOWN_TIMEOUT=300
# libvirt's own directory for the saved states: /var/lib/libvirt/qemu/save
```

`ON_SHUTDOWN=suspend` is the save; `ON_SHUTDOWN=shutdown` shuts each VM down,
which is the drain by another name. `SHUTDOWN_TIMEOUT` bounds the save: a VM
whose guest does not answer in it is left running, so make it longer than the
guest takes to write its memory out.

**Size the save directory for the VM's memory.** The file is about the size of
the guest's used memory, so a 32 GiB maximum needs 32 GiB of room under
`/var/lib/libvirt/qemu/save` and it is written on the hypervisor's own disk.
Check before the first save with `df /var/lib/libvirt/qemu/save`, and move the
directory to a filesystem with the room on a host that lacks it.

### Restart

```
sudo systemctl start libvirt-guests.service   # saves each running VM now
sudo reboot                                   # the hypervisor's own kernel
```

The save happens at the reboot itself when `ON_SHUTDOWN=suspend` is set; the
first line is for saving before a restart of `libvirtd` alone, or for checking
the step by hand. Then bring the VM back if the boot did not, and sync its
clock:

```
virsh start tehas-tehas                   # not needed when libvirt-guests restored it
virsh domtime --sync tehas-tehas          # the guest's clock to the hypervisor's
```

`virsh domtime --sync` needs the guest agent; without it, `chronyc makestep`
in the guest. A restored VM's clock is the one it had when it stopped, and the
factory's work is full of stamps: the dispatcher compares label events, the
park records when a cycle stopped, and a token older than its own hour is
refused by GitHub. So sync it, and then check the units:

```
systemctl --user list-units 'tehas-*' --state=active   # the timers and the containers
tehas status
```

The docs server and the edge router are `docker compose` containers and do not
come back with the user's units; the lines are on [Services](/host#services).

### What the cycles do through it

Cycles continue where they were: the containers are inside the VM, so they are
saved with it, and a gate that was running resumes running. Two things are not
saved with them, and both are handled outside tehas:

- **The model request in flight.** A request open at the save is still open
  when the guest wakes minutes later, and by then the endpoint has stopped
  serving it and the connection has gone. Pi's provider retry takes the failed
  call and asks again; a retry that cannot be made parks the cycle as any
  provider failure does, with `kind` `provider`, and the dispatcher starts it
  again once, without a tick, when the endpoint answers. So one
  request is lost, and the cycle loses the minutes of that one call, not the
  cycle.
- **The GitHub call in flight**, same shape: the call fails, the host retries
  it, and where a label change never happened the poll sorts the label out on a
  later pass.

What a save does not do is wait: it writes memory while the guest is stopped,
so the clock gap is real and every timeout inside the guest counts it. A cycle
that was minutes from its deadline keeps that deadline in the same absolute
time, so a long save can end a cycle at its deadline that would otherwise have
finished its step. A drain is the right answer when you cannot afford that.

## When a drain is still needed

The live paths above do not cover everything. Drain first, as in
[Limit and drain](/operating#limit-and-drain), for:

- **Raising the maximum, or changing the topology.** A `vcpu` maximum, a
  `maxMemory` maximum, its slot count, or a topology whose numbers no longer
  multiply out: all of them need the VM powered off, and a cycle's container
  killed by a power off is a cycle lost.
- **Changing the machine type**, such as `pc-q35-*` to a newer one, or the CPU
  model. The guest sees a different machine and its drivers, its network
  interface names and its disk names may move, and Docker keeps its state on
  those disks.
- **Moving the disk to another pool**, or resizing the backing volume. The
  containers, the images and every workspace are on it; a container whose
  backing files move under it is not a container any more.
- **Anything that takes the VM off the network for longer than a poll's
  patience**, or that stops Docker rather than the guest: the running cycles
  depend on both.

A drain is three commands and it never stops what runs:

```
tehas drain --wait 3600     # start nothing new, wait for the running cycles
# the work on the hypervisor
tehas limit clear           # afterwards
```

Drained means no poll can still start a cycle, and drain waits for the poll
that is running before it says none is, so a cycle a poll had decided to start
is counted rather than left behind.

## What was ruled out

Saving one container and bringing it back, rather than the whole VM: Docker's
`docker checkpoint create`, `criu`, and the Kubernetes shapes of the same idea.
Ruled out for three reasons, each of which is enough on its own:

- **It is experimental in Docker.** `--experimental` has to be on for
  `docker checkpoint create` to answer at all, criu must be installed on the
  host, and the feature is not a supported artefact. Nothing in tehas would be
  able to rely on it.
- **It does not carry the connections that matter.** A cycle's container holds
  open sockets to the model endpoint and to GitHub. criu can only save a
  connection whose both ends it can save, so those go, and what the agent gets
  back is a process whose conversation has stopped mid-call: the same failure
  as the VM save above, but with tehas code added to reach it.
- **The VM already does it.** A `libvirt-guests` save carries every container
  on the host at once, with no tehas code, no daemon flag and no per-cycle
  coordination. Saving a cycle is a smaller job for the same result, so the
  extra machinery buys nothing.

What tehas does have for a cycle that must not be in the way is
[drain](/operating#limit-and-drain), and a pause, which stops new starts and
lets what runs finish.

## See also

- [The host](/host): the machine's layout, the slice and what its containers
  are given, [the slice](/host#the-slice) and
  [limits and concurrency](/host#limits-and-concurrency)
- [Operating tehas](/operating#limit-and-drain): drain, pause and resume
- [Features](/features#host-management): how far each of these is proven
