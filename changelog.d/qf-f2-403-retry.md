### Changes

- A poll's queue read is tried once more when GitHub answers a bare 403, the one retry the queue lost when the read moved onto the ports; a 403 that names its permission (`Forbidden`) is still a refusal and is not tried again.
