# README

First, any updates to `pintrail.service` need to be copied to `/etc/systemd/system/pintrail.service` on the target machine. This is not automated, but it is a simple copy operation. This is a `sudo` operation, so you will need to have the appropriate permissions.

After that, to run the service with `systemctl`:

```bash
sudo systemctl daemon-reload
sudo systemctl enable pintrail
sudo systemctl start pintrail # or restart
sudo systemctl status pintrail.service -l
```

Debugging:

```bash
sudo systemctl status pintrail.service -l --no-pager
sudo journalctl -u pintrail.service -b -n 100 --no-pager
```

## NOTES

It took a real effort to get the system up and running using `systemd`. The real problem boiled down to boot time races with docker networking. Although I would like to learn more about `systemd` and how to use it properly, I don't have the time to do so right now. The current `pintrail.service` configuration file does what it needs to do.