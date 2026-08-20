# DROID Circuit Examples

One minimal example of every DROID circuit type, extracted from real `.ini` patch files.

---

## Logic

### Compare
```ini
[compare]
    input = I1
    compare = 1
    ifequal = 1
    else = 0
```

### Copy
```ini
[copy]
    input = _ENV1_DECAY_POT_ABSBIPOLAR * -1 + _DECAY_MIN
    output = _ENV1_DECAY_POT
```

### Ifequal
```ini
[ifequal]
    input1 = _TUNING_MODE
    input2 = 0
    ifequal = _LOW_BASS_PITCH_UNTUNED
    else = _TUNING_PITCH
    output = _LOW_BASS_OUT
```

### Logic
```ini
[logic]
    input1 = _FADE_LED
    input2 = _FADE_BLINK
    or = L1.9
```

### Math
```ini
[math]
    input1 = _MIDI_CLOCK_PITCH
    input2 = _WILD_CLOCK_PITCH
    quotient = _CLOCK_QUOT
    difference = _CLOCK_DIFF
```

### Multicompare
```ini
[multicompare]
    input = _VOICE_SELECT
    compare1 = 0
    ifequal1 = _VOICE_1_ACTIVE
    compare2 = 1
    ifequal2 = _VOICE_2_ACTIVE
```

### Select
```ini
[select]
    input = _LED3_7_MODHUB
    select = _MOD_CTRL
    output = L3.7
```

> **No examples found in patches:** `Adc`, `Dac`, `Explin`

---

## Sequencing

### Algoquencer
```ini
[algoquencer]
    clock = I1
    pitchlow = 0
    pitchhigh = _SAMPLE1_LEVEL
    length = 16
    reroll = _SAMPLE1_DEJAVU
    dejavu = _SAMPLE1_DEJAVU
    pitch = _SAMPLE1_RANDOM
```

### Arpeggio
```ini
[arpeggio]
    clock = I1
    direction = _DIRECTION
    pingpong = _PINGPONG
    butterfly = _BUTTERFLY
    octaves = _OCTAVES
    drop = _DROP
    startnote = _STARTNOTE
    pattern = P1.2 * 6
    pitch = 1V
    range = 2V
    root = 0
    degree = _DEGREE
    select1 = 1
    select3 = 1
    select5 = 1
    select7 = _SCALE
    select9 = _SCALE
    select11 = _SCALE
    select13 = _SCALE
    output = O4
```

### Euklid
```ini
[euklid]
    clock = _DIVIDED_CLOCK
    reset = _RESET
    length = _EUKLID_LENGTH
    beats = _EUKLID_BEATS
    offset = _EUKLID_OFFSET
    output = _GATE_EUKLID
```

### Motoquencer
```ini
[motoquencer]
    clock = _CLOCK
    cv = O1
    gate = O5
```

### Sequencer
```ini
[sequencer]
    clock = G1.1
    reset = G1.2
    pitch1 = 0
    pitch2 = 1
    pitchoutput = _MODULATION_OFFBEATS
```

> **No examples found in patches:** `Encoquencer`, `Polytool`

---

## Clock

### Bernoulli
```ini
[bernoulli]
    clock = _CLOCK
    probability = _BERN_PROB
    output = _BERN_OUT
```

### Burst
```ini
[burst]
    trigger = _ENV1_SWITCH
    hz = 5
    count = 5
    output = _ENV1_SWITCH_LED
```

### Clocktool
```ini
[clocktool]
    clock = _MODULAR1CLOCK
    gatelength = 0.1
    output = _MODULAR1CLOCK_GATE
```

### Flipflop
```ini
[flipflop]
    toggle = _ENV1_SWITCH
    output = _ENV1_LIN_EXP
```

### Gatetool
```ini
[gatetool]
    outputtrigger = _PULSE
    inputedge = G1.1
```

### Once
```ini
[once]
    delay = 0.01
    trigger = _RESET_MIDIOUT_INI
```

### Timing
```ini
[timing]
    clock = _MODULAR2CLOCK
    reset = _RESET_MODULAR
    timing1 = 0
    timing2 = 0.3
    output = _MODULAR3CLOCK
```

### Triggerdelay
```ini
[triggerdelay]
    input = O1
    output = O2
    clock = I1
    delay = _DELAY * 2
```

---

## UI

### Button
```ini
[button]
    button = B1.1
    longpress = _ENV1_SWITCH
```

### Buttongroup
```ini
[buttongroup]
    button1 = B1.1
    button2 = B1.2
    button3 = B1.3
    button4 = B1.4
    led1 = L1.1
    led2 = L1.2
    led3 = L1.3
    led4 = L1.4
    output = _PRESET
```

### Encoder
```ini
[encoder]
    encoder = E1.1
    select = _TUNING_MODE
    discrete = 7
    outputscale = 1V
    output = _FM_OPERATOR_TUNING_PITCH
```

### Faderbank
```ini
[faderbank]
    output1 = O1
    output2 = O2
    output3 = O3
    output4 = O4
    preset = _PRESET
```

### Fadermatrix
```ini
[fadermatrix]
    rowcolumn = _ROWCOLUMN
    ledcolor1 = 0.2
    ledcolor2 = 0.4
    ledcolor3 = 0.6
    ledcolor4 = 0.8
    output11 = _ATTACK_1
    output12 = _DECAY_1
    output13 = _SUSTAIN_1
    output14 = _RELEASE_1
    output21 = _ATTACK_2
    output22 = _DECAY_2
    output23 = _SUSTAIN_2
    output24 = _RELEASE_2
    output31 = _ATTACK_3
    output32 = _DECAY_3
    output33 = _SUSTAIN_3
    output34 = _RELEASE_3
```

### Motorfader
```ini
[motorfader]
    select = L2.1
    fader = 1
    output = O1
```

### Notebuttons
```ini
[notebuttons]
    preset = _PRESET
    select = _STEP1_CURSOR * _NO_GLOBAL_TRANS_MODE
    button1 = B2.1
    button2 = B2.2
    button3 = B2.3
    button4 = B2.4
    button5 = B2.5
    button6 = B2.6
    button7 = B2.7
    button8 = B2.8
    button9 = B2.9
    button10 = B2.10
    button11 = B2.11
    button12 = B2.12
    led1 = L2.1
    led2 = L2.2
    led3 = L2.3
    led4 = L2.4
```

### Nudge
```ini
[nudge]
    buttonup = B2.30
    buttondown = B2.29
    amount = 1
    startvalue = 1
    minimum = 1
    maximum = _SEQUENCE_NUM_STEPS
    wrap = 1
    ledup = L2.30
    leddown = L2.29
    output = _MANUAL_CURSOR_POSITION
```

### Pot
```ini
[pot]
    pot = _TIDE_SPEED_POT
    slope = 2
    output = _TIDE_SPEED
```

### Unusedfaders
```ini
[unusedfaders]
    select = _GLOBAL_FADER_SELECTION
    selectat = 97
    firstfader = 9
    numfaders = 8
```

> **No examples found in patches:** `Encoderbank`

---

## Pitch

### Calibrator
```ini
[calibrator]
    input = _CH_SLEWED_1 + _OCT_SWITCH_1
    nudgeup = _CAL_UP_1
    nudgedown = _CAL_DOWN_1
    correction = _CAL_CORRECTION_1
    output = _CALIBRATED_CH_1
    nudgeamount = 0.01
    clearhere = _CAL_CLEARHERE_1
```

### Chord
```ini
[chord]
    root = _ROOT
    degree = _SCALE
    spread = _CHORD_SPREAD
    pitch = _CHORD_PITCH_COMPENSATED + _CHORD_PITCH_RANDOM_SYNCED
    inversion = _CHORD_INVERSION
    select1 = _SEL_ROOT
    select3 = _SEL_3RD
    select5 = _SEL_5TH
    select7 = _SEL_7TH
    select9 = _SEL_9TH
    select11 = _SEL_11TH
    select13 = _SEL_13TH
    tuningmode = _TUNING_MODE
    tuningpitch = _TUNING_PITCH
    output1 = _CH_OUT_1
    output2 = _CH_OUT_2
    output3 = _CH_OUT_3
```

### Detune
```ini
[detune]
    input1 = _VIBRATED_CH_1
    input2 = _VIBRATED_CH_2
    input3 = _VIBRATED_CH_3
    input4 = _VIBRATED_CH_4
    input5 = _VIBRATED_LB
    detune = P1.6 * -0.6
    output1 = O5
    output2 = O6
    output3 = O7
    output4 = O8
    output5 = O4
```

### Fold
```ini
[fold]
    input = I7
    minimum = 0
    maximum = 0.5
    foldby = 0.01
    output = _LFO1_CVIN
```

### Minifonion
```ini
[minifonion]
    input = _SAMPLE1_RANDOM
    trigger = I1
    root = 0
    degree = 7
    select1 = 1
    select3 = 1
    select5 = 1
    select7 = 1
    select9 = 0
    select11 = 1
    select13 = 0
    output = _SAMPLE1_QUANTIESED
```

### Superjust
```ini
[superjust]
    input1 = _RESET_MIDIOUT
    input2 = _RESET_MODULAR
    input3 = _RESET_MODULAR
    input4 = _STABLE_CLOCK
    tuningmode = 0
    tuningpitch = 0
    bypass = 0
    output1 = R22
    output2 = R20
    output3 = G1.4
    output4 = R17
    input5 = _CLOCK_MODULAR
```

### Vcotuner
```ini
[vcotuner]
    tuningnote = 0
    ledflat = _FLAT
    ledsharp = _SHARP
    intune = _INTUNE
    vcofound = _VCOFOUND
```

> **No examples found in patches:** `Octave`, `Sinfonionlink`

---

## CV

### Case
```ini
[case]
    case1 = _MIDI_INPUT_CLOCK_PRESENT
    value1 = _MIDI_INPUT_CLOCK
    case2 = _INT_CLOCK_UNMUTED
    value2 = _INT_CLOCK
    case3 = _EXTERNAL_CLOCK_PRESENT
    value3 = I1
    output = _CLOCK_UNSWUNG
```

### Crossfader
```ini
[crossfader]
    input1 = _FM_DEPTH_POT * 0.7
    input2 = _FM_ENVELOPE * 0.7
    fade = _FM_ENV_POT
    output = _FM_DEPTH_NO_TUNING
```

### Cvlooper
```ini
[cvlooper]
    clock = _CLOCK
    reset = _RESET
    tapespeed = 0.2
    cvin = _RIBBON_PITCH_INPUT
    gatein = _RIBBON_GATE_BEFORE_LOOPER
    cvout = _RIBBON_PITCH_LOOPED
    gateout = _RIBBON_GATE
    loopswitch = _RIBBON_LOOP_SWITCH
    overlay = 1
    bypass = _RIBBON_LOOP_BYPASS + _CLOCK_STOPPED
    overdub = _RIBBON_LOOP_OVERDUB
    length = _LOOP_LENGTH
```

### Matrixmixer
```ini
[matrixmixer]
    input1 = _CC1
    input2 = _CC2
    input3 = _CC3
    input4 = _CC4
    mix11 = _CCPOT1T1
    mix12 = _CCPOT2T1
    mix13 = _CCPOT3T1
    mix14 = _CCPOT4T1
    output1 = _MIXOUT1
```

### Mixer
```ini
[mixer]
    input1 = _ENV1_SWITCH_LED * -0.5 + _LED1
    input2 = _ENV1_LIN_EXP * _LONGPRESS_LED
    output = L1.1
```

### Quantizer
```ini
[quantizer]
    input = I5
    output = _PITCH_SEQUENCER_1
```

### Recorder
```ini
[recorder]
    cvin = _CUTOFF_POT
    cvout = _CH1_FIXED_CUTOFF
    loop = 1
    mode = _FM_LOOP_RECORDING + 1
    bypass = _FM_LOOP_BYPASS
    recordled = L3.27
```

### Sample
```ini
[sample]
    input = _FM_RATIO_SWITCH_UNCLOCKED
    sample = G1.5
    output = _FM_RATIO_SWITCH
```

### Slew
```ini
[slew]
    input = _SAMPLE1_QUANTIESED
    slew = _SAMPLE1_SLEW
    linear = O5
```

### Switch
```ini
[switch]
    input2 = _STABLE_CLOCK
    input4 = _WILD_CLOCK
    offset = S1.3
    output1 = _CLOCK_MODULAR
    input1 = _WILD_CLOCK
```

> **No examples found in patches:** `Delay`, `Queue`

---

## Modulation

### Contour
```ini
[contour]
    gate = _ENV1_GATE
    attack = _ENV1_ATTACK_POT
    decay = _ENV1_DECAY_POT
    sustain = _ENV1_SUSTAIN_POT
    release = _ENV1_RELEASE_POT
    output = _ENV1_OUT
```

### Lfo
```ini
[lfo]
    hz = 20 * P1.1
    square = _CLOCK
```

### Random
```ini
[random]
    clock = _LFO1_CLOCK
    probability = 0.5
    output = _LFO1_RANDOM
```

### Spring
```ini
[spring]
    mass = _CCPOT17T1
    springforce = 1
    friction = _CCPOT21T1
    position = _SPRINGPOS1
    shove = _SHOVESPRING1
    velocity = _SPRINGVELOCITY1
    speed = _CCPOT18T1
    startvelocity = _CCPOT22T1
    reset = _RESET
```

> **No examples found in patches:** `Transient`

---

## Other

### Droid
```ini
[droid]
    ledbrightness = 0.5
```

### Outputcalibrator
```ini
[outputcalibrator]
    output = _CHANNEL + 1
    referencepoint = _REFPOINT
    save = B1.4
    cancel = B1.1
    loaddefaults = B1.2
    nudgeup = _UP
    nudgedown = _DOWN
    dirty = _DIRTY
    calibration = _CAL
    uncalibrated = _UNCAL
```

---

## MIDI

### Firefacecontrol
```ini
[firefacecontrol]
    outputlevel1 = _OUTPUT_MAIN * _FADE_LEVEL
    outputlevel3 = _OUTPUT_MAGNETO
    outputlevel5 = _OUTPUT_STARLAB
    outputlevel7 = _OUTPUT_ZDSP
    outputlevel9 = _OUTPUT_PHONES
    outputlevel11 = _OUTPUT_REVERB
    outputmix1in1 = _VOLUME_EFFECTS
    outputmix1in3 = _VOLUME_LB
    outputmix1in4 = _GAIN_INPUT_BD * _MAIN_INPUT_BD
    outputmix1in5 = _GAIN_INPUT_DR * _MAIN_INPUT_DR
    outputmix1in7 = _VOLUME_CH
    outputmix1in9 = _VOLUME_TB
    outputmix1in10 = _GAIN_INPUT_LD * _MAIN_INPUT_LD
    outputmix1in11 = _GAIN_INPUT_OR * _MAIN_INPUT_OR
    outputmix1in12 = _GAIN_INPUT_AR * _MAIN_INPUT_AR
```

### Midiin
```ini
[midiin]
    clock = _MIDI_CLOCK
    start = _MIDI_START
    stop = _MIDI_STOP
    continue = _MIDI_CONTINUE
```

### Midiout
```ini
[midiout]
    clock = _CLOCK_MIDIOUT
    channel = 1
    usb = 0
    trs = 1
    notegate1 = _RESET_MIDIOUT
    note1 = 24
```

### Midithrough
```ini
[midithrough]
    totrs = 2
    fromusb = 1
```

> **No examples found in patches:** `Midifileplayer`

---

## Deprecated

### Switchedpot
```ini
[switchedpot]
    pot = P1.1
    switch1 = _LAYER_A
    switch2 = _LAYER_B
    output1 = _P_SPEED
    output2 = _P_ATTACK
```

### Togglebutton
```ini
[togglebutton]
    button = B3.8
    led = L3.8
    offvalue = 0
    onvalue = 1
    output = _DELAY
```

> **No examples found in patches:** `Fourstatebutton`, `Notchedpot`

---

## Complete Patch Example

### Arpeggiator (`droid-blue-7/patches/arpeggio1.ini`)

A complete working patch: algorithmic melody generator with one P2B8 controller.

```ini
# LIBRARY: number=1, version=1.0

# Arpeggiator with one P2B8
# The arpeggiator is an algorithmic melody generator. Usually there
# is no randomization but several algorithms, which can be combined
# in order to create complex and interesting patterns. This version
# just needs one P2B8 but still allows to select and combine all of
# the arpeggiator circuit's algorithms. It can be clocked externally
# or internally.

# INPUTS:
#  I1: [CLK] optional external clock

# OUTPUTS:
#  O1: [CLK] internal clock
#  O3: [ENV] envelope
#  O4: [V/O] pitch (V/oct)

# CONTROLLER 1:
#  P1.1: [SPE] Speed of internal clock
#  P1.2: [PAT] Arpeggio pattern (0 ... 6)
#  B1.1: [DIR] Direction up (off) or down (on)
#  B1.2: [PNG] Enable ping pong: go forth and back
#  B1.3: [FLY] Switch on butterfly mode
#  B1.4: [OCT] Set octaving mode in three state: off, up down
#  B1.5: [DRP] When on, drop some of the notes (off / x. / xx. / x..)
#  B1.6: [STA] Enforce start note: off / 3rd / 5th / 7th
#  B1.7: [TRI] Switch between all notes of the scale (off) and just the triad (on)
#  B1.8: [MIN] Switch between minor (off) and major (on)

[p2b8]

# -------------------------------------------------
# Master clock
# -------------------------------------------------

# Send the clock the the normalization of input 1. In the
# rest of the patch I1 will be used as input, and you can
# override the internal clock by patching an external in.
[lfo]
    hz = 40 * P1.1
    square = N1

# Make the clock also available on O1
[copy]
    input = N1
    output = O3

[copy]
    input = I1
    output = O1

# -------------------------------------------------
# Buttons
# -------------------------------------------------

[button]
    button = B1.1
    led = L1.1
    output = _DIRECTION

[button]
    button = B1.2
    led = L1.2
    output = _PINGPONG

[button]
    button = B1.3
    led = L1.3
    output = _BUTTERFLY

[button]
    button = B1.4
    led = L1.4
    value1 = 0
    value2 = 1
    value3 = 2
    output = _OCTAVES

[button]
    button = B1.5
    led = L1.5
    value1 = 0
    value2 = 1
    value3 = 2
    value4 = 3
    output = _DROP

[button]
    button = B1.6
    led = L1.6
    value1 = 0
    value2 = 3
    value3 = 5
    value4 = 7
    output = _STARTNOTE

[button]
    button = B1.7
    led = L1.7
    onvalue = 0
    offvalue = 1
    output = _SCALE

[button]
    button = B1.8
    led = L1.8
    offvalue = 7 # minor
    onvalue = 1 # major
    output = _DEGREE

# -------------------------------------------------
# Arpeggiator
# -------------------------------------------------

[arpeggio]
    clock = I1
    direction = _DIRECTION
    pingpong = _PINGPONG
    butterfly = _BUTTERFLY
    octaves = _OCTAVES
    drop = _DROP
    startnote = _STARTNOTE
    pattern = P1.2 * 6
    pitch = 1V
    range = 2V
    root = 0 # 0 means C
    degree = _DEGREE
    select1 = 1
    select3 = 1
    select5 = 1
    select7 = _SCALE
    select9 = _SCALE
    select11 = _SCALE
    select13 = _SCALE
    output = O4

# -------------------------------------------------
# Envelope generator
# -------------------------------------------------

# This envelope is optional. You can either use the output O1 for triggering a synth voice that has its own envelope.
# Or you use output 3 and use DROID also as the envelope generator for you synth.
# In that case you just need VCO, VCF and optionally a VCA.
[contour]
    gate = I1
    attack = 0.1
    sustain = 0.7
    output = O3
```

---

## Summary

| Category | Circuits with examples | No example found |
|----------|----------------------|------------------|
| **Logic** | Compare, Copy, Ifequal, Logic, Math, Multicompare, Select | Adc, Dac, Explin |
| **Sequencing** | Algoquencer, Arpeggio, Euklid, Motoquencer, Sequencer | Encoquencer, Polytool |
| **Clock** | Bernoulli, Burst, Clocktool, Flipflop, Gatetool, Once, Timing, Triggerdelay | — |
| **UI** | Button, Buttongroup, Encoder, Faderbank, Fadermatrix, Motorfader, Notebuttons, Nudge, Pot, Unusedfaders | Encoderbank |
| **Pitch** | Calibrator, Chord, Detune, Fold, Minifonion, Superjust, Vcotuner | Octave, Sinfonionlink |
| **CV** | Case, Crossfader, Cvlooper, Matrixmixer, Mixer, Quantizer, Recorder, Sample, Slew, Switch | Delay, Queue |
| **Modulation** | Contour, Lfo, Random, Spring | Transient |
| **Other** | Droid, Outputcalibrator | — |
| **MIDI** | Firefacecontrol, Midiin, Midiout, Midithrough | Midifileplayer |
| **Deprecated** | Switchedpot, Togglebutton | Fourstatebutton, Notchedpot |

**Total:** 59 circuits with examples, 15 without examples in available `.ini` files.
