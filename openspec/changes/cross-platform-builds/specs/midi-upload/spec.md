## MODIFIED Requirements

### Requirement: Both transports, explicit failures

USB-MIDI (X7 autodetect) and DIN MIDI SHALL both be reachable on every supported platform; missing tools or hardware SHALL surface as a status error naming what is absent. Port discovery SHALL use `amidi -l` where `amidi` exists and SHALL fall back to `sendmidi list` when it does not, so a machine with only `sendmidi` installed still offers ports. Tool detection SHALL resolve platform executable extensions, so `sendmidi.exe` on Windows counts as present. Because the two tools address ports differently (`hw:1,0` versus a device name), the confirm modal and the status SHALL name the tool that carries the send.

#### Scenario: No hardware connected

- **WHEN** no DROID port is detected
- **THEN** the modal offers no `y` path and the status names the missing hardware or tool

#### Scenario: sendmidi-only machine

- **WHEN** `amidi` is absent, `sendmidi` is present, and `sendmidi list` names a DROID device
- **THEN** that device is offered as the USB-MIDI port and the modal names `sendmidi` as the transport

#### Scenario: Windows executable detection

- **WHEN** `sendmidi.exe` sits on `PATH` on Windows
- **THEN** the tool is reported present and a send can be offered
