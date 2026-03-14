/* AiOS — AC97 Audio Controller Driver
   Intel ICH 82801AA AC97 for QEMU.
   DMA-based PCM playback and capture over PCI bus master. */

#include "include/ac97.h"
#include "include/pci.h"
#include "include/io.h"
#include "include/string.h"
#include "include/stdio.h"
#include "include/arch/cc.h"   /* u32_t */

#include "include/debug_log.h"
extern u32_t sys_now(void);
extern void net_poll(void);

#if AIOS_DEBUG
#define AC97_DBG(fmt, ...) do { \
    char _d[120]; \
    snprintf(_d, sizeof(_d), "[%lu] ac97: " fmt, (unsigned long)sys_now(), ##__VA_ARGS__); \
    dbg_log(_d); \
} while(0)
#else
#define AC97_DBG(fmt, ...) ((void)0)
#endif

/* ========================================================================= */
/* I/O base addresses                                                        */
/* ========================================================================= */

static uint16_t nam_bar;    /* BAR0: Native Audio Mixer */
static uint16_t nabm_bar;   /* BAR1: Native Audio Bus Master */
static int ac97_ready = 0;

/* ========================================================================= */
/* NAM (Mixer) register offsets                                              */
/* ========================================================================= */

#define NAM_RESET           0x00
#define NAM_MASTER_VOL      0x02
#define NAM_AUX_OUT_VOL     0x04
#define NAM_MONO_VOL        0x06
#define NAM_MIC_VOL         0x0E
#define NAM_LINE_IN_VOL     0x10
#define NAM_PCM_OUT_VOL     0x18
#define NAM_REC_SELECT      0x1A
#define NAM_REC_GAIN        0x1C
#define NAM_EXT_AUDIO_ID    0x28
#define NAM_EXT_AUDIO_CTRL  0x2A
#define NAM_PCM_FRONT_RATE  0x2C
#define NAM_PCM_LR_ADC_RATE 0x32

/* ========================================================================= */
/* NABM (Bus Master) register offsets                                        */
/* ========================================================================= */

#define NABM_PCM_IN         0x00
#define NABM_PCM_OUT        0x10
#define NABM_MIC_IN         0x20
#define NABM_GLOB_CTRL      0x2C
#define NABM_GLOB_STS       0x30

/* Per-channel register offsets (add to channel base) */
#define CH_BDBAR    0x00    /* Buffer Descriptor Base Address - 32-bit */
#define CH_CIV      0x04    /* Current Index Value - 8-bit */
#define CH_LVI      0x05    /* Last Valid Index - 8-bit */
#define CH_SR       0x06    /* Status Register - 16-bit */
#define CH_PICB     0x08    /* Position In Current Buffer - 16-bit */
#define CH_PIV      0x0A    /* Prefetched Index Value - 8-bit */
#define CH_CR       0x0B    /* Control Register - 8-bit */

/* Status Register bits */
#define SR_DCH      (1 << 0)    /* DMA Controller Halted */
#define SR_CELV     (1 << 1)    /* Current Equals Last Valid */
#define SR_LVBCI    (1 << 2)    /* Last Valid Buffer Completion */
#define SR_BCIS     (1 << 3)    /* Buffer Completion Interrupt Status */
#define SR_FIFOE    (1 << 4)    /* FIFO Error */

/* Control Register bits */
#define CR_RPBM     (1 << 0)    /* Run/Pause Bus Master */
#define CR_RR       (1 << 1)    /* Reset Registers */
#define CR_LVBIE    (1 << 2)    /* Last Valid Buffer Interrupt Enable */
#define CR_IOCE     (1 << 3)    /* Interrupt On Completion Enable */
#define CR_FEIE     (1 << 4)    /* FIFO Error Interrupt Enable */

/* Extended Audio bits */
#define EXT_VRA     (1 << 0)    /* Variable Rate Audio */

/* ========================================================================= */
/* Buffer Descriptor List (BDL)                                              */
/* ========================================================================= */

typedef struct {
    uint32_t addr;      /* Physical buffer address */
    uint16_t samples;   /* Number of 16-bit samples */
    uint16_t flags;     /* Bit 15: IOC, Bit 14: BUP */
} __attribute__((packed)) ac97_bdl_t;

#define BDL_IOC     (1 << 15)   /* Interrupt On Completion */
#define BDL_BUP     (1 << 14)   /* Buffer Underrun Policy */

#define AC97_MAX_BDL    32

/* Static BDL arrays — must be in identity-mapped DMA-accessible memory */
static ac97_bdl_t out_bdl[AC97_MAX_BDL] __attribute__((aligned(8)));
static ac97_bdl_t in_bdl[AC97_MAX_BDL] __attribute__((aligned(8)));

/* ========================================================================= */
/* Helpers                                                                   */
/* ========================================================================= */

static inline void nam_write16(uint16_t reg, uint16_t val) {
    outw(nam_bar + reg, val);
}
static inline uint16_t nam_read16(uint16_t reg) {
    return inw(nam_bar + reg);
}
static inline void nabm_write8(uint16_t off, uint8_t val) {
    outb(nabm_bar + off, val);
}
static inline uint8_t nabm_read8(uint16_t off) {
    return inb(nabm_bar + off);
}
static inline void nabm_write16(uint16_t off, uint16_t val) {
    outw(nabm_bar + off, val);
}
static inline uint16_t nabm_read16(uint16_t off) {
    return inw(nabm_bar + off);
}
static inline void nabm_write32(uint16_t off, uint32_t val) {
    outl(nabm_bar + off, val);
}
static inline uint32_t nabm_read32(uint16_t off) {
    return inl(nabm_bar + off);
}

/* ========================================================================= */
/* Channel control                                                           */
/* ========================================================================= */

static void ac97_reset_channel(uint16_t ch_base) {
    /* Set reset bit */
    nabm_write8(ch_base + CH_CR, CR_RR);
    /* Wait for reset to clear */
    for (int i = 0; i < 100000; i++) {
        if (!(nabm_read8(ch_base + CH_CR) & CR_RR)) break;
    }
    /* Clear all status bits */
    nabm_write16(ch_base + CH_SR, SR_LVBCI | SR_BCIS | SR_FIFOE);
}

/* ========================================================================= */
/* Public API                                                                */
/* ========================================================================= */

int ac97_init(void) {
    pci_device_t dev;
    if (!pci_find_device(AC97_VENDOR_ID, AC97_DEVICE_ID, &dev)) {
        AC97_DBG("AC97 not found on PCI bus\n");
        return -1;
    }

    /* BAR0 = NAM (I/O space), BAR1 = NABM (I/O space) */
    nam_bar  = dev.bar[0] & ~3u;
    nabm_bar = dev.bar[1] & ~3u;

    AC97_DBG("found PCI %02x:%02x NAM=0x%x NABM=0x%x\n",
             dev.bus, dev.dev, nam_bar, nabm_bar);

    /* Enable I/O space access + bus mastering */
    uint16_t cmd = pci_read16(dev.bus, dev.dev, dev.func, 0x04);
    cmd |= (1 << 0) | (1 << 2);  /* I/O Space + Bus Master */
    pci_write16(dev.bus, dev.dev, dev.func, 0x04, cmd);

    /* Cold reset via Global Control */
    nabm_write32(NABM_GLOB_CTRL, 0x02);  /* Cold reset */
    for (volatile int i = 0; i < 1000000; i++);  /* Wait for codec */

    /* Check codec is ready (Global Status bit 8 = Primary Codec Ready) */
    uint32_t gs = nabm_read32(NABM_GLOB_STS);
    if (!(gs & (1 << 8))) {
        /* Try warm reset */
        nabm_write32(NABM_GLOB_CTRL, 0x04);
        for (volatile int i = 0; i < 1000000; i++);
        gs = nabm_read32(NABM_GLOB_STS);
        if (!(gs & (1 << 8))) {
            AC97_DBG("codec not ready (GS=0x%lx)\n", (unsigned long)gs);
            return -1;
        }
    }

    /* Reset codec via NAM */
    nam_write16(NAM_RESET, 0x0000);
    for (volatile int i = 0; i < 100000; i++);

    /* Unmute and set volumes to max */
    nam_write16(NAM_MASTER_VOL,  0x0000);  /* 0dB, unmuted */
    nam_write16(NAM_AUX_OUT_VOL, 0x0000);
    nam_write16(NAM_MONO_VOL,    0x0000);
    nam_write16(NAM_PCM_OUT_VOL, 0x0000);  /* PCM at max */
    nam_write16(NAM_MIC_VOL,     0x0000);  /* Mic unmuted */
    nam_write16(NAM_LINE_IN_VOL, 0x0000);
    nam_write16(NAM_REC_GAIN,    0x0000);

    /* Set record source to microphone (MIC = 0x0000) */
    nam_write16(NAM_REC_SELECT, 0x0000);

    /* Enable Variable Rate Audio if supported */
    uint16_t ext_id = nam_read16(NAM_EXT_AUDIO_ID);
    if (ext_id & EXT_VRA) {
        uint16_t ext_ctrl = nam_read16(NAM_EXT_AUDIO_CTRL);
        ext_ctrl |= EXT_VRA;
        nam_write16(NAM_EXT_AUDIO_CTRL, ext_ctrl);
        AC97_DBG("variable rate audio enabled\n");
    } else {
        AC97_DBG("VRA not supported, using 48kHz\n");
    }

    /* Default: 48kHz output and input */
    nam_write16(NAM_PCM_FRONT_RATE, 48000);
    nam_write16(NAM_PCM_LR_ADC_RATE, 48000);

    /* Reset DMA channels */
    ac97_reset_channel(NABM_PCM_OUT);
    ac97_reset_channel(NABM_PCM_IN);
    ac97_reset_channel(NABM_MIC_IN);

    ac97_ready = 1;
    AC97_DBG("init OK (VRA=%d)\n", !!(ext_id & EXT_VRA));
    return 0;
}

int ac97_is_ready(void) {
    return ac97_ready;
}

void ac97_set_master_volume(uint16_t vol) {
    if (!ac97_ready) return;
    nam_write16(NAM_MASTER_VOL, vol);
}

void ac97_set_pcm_volume(uint16_t vol) {
    if (!ac97_ready) return;
    nam_write16(NAM_PCM_OUT_VOL, vol);
}

int ac97_set_output_rate(uint16_t rate) {
    if (!ac97_ready) return -1;
    nam_write16(NAM_PCM_FRONT_RATE, rate);
    uint16_t actual = nam_read16(NAM_PCM_FRONT_RATE);
    AC97_DBG("output rate: requested %d, got %d\n", rate, actual);
    return (actual == rate) ? 0 : -1;
}

int ac97_set_input_rate(uint16_t rate) {
    if (!ac97_ready) return -1;
    nam_write16(NAM_PCM_LR_ADC_RATE, rate);
    uint16_t actual = nam_read16(NAM_PCM_LR_ADC_RATE);
    AC97_DBG("input rate: requested %d, got %d\n", rate, actual);
    return (actual == rate) ? 0 : -1;
}

/* ========================================================================= */
/* Playback                                                                  */
/* ========================================================================= */

/* Max samples per BDL entry (16-bit words) */
#define MAX_BDL_SAMPLES 0xFFFE

/* Static stereo conversion buffer — used when playing mono audio.
   64KB allows ~1.3s at 24kHz mono→stereo (16384 stereo frames). */
static int16_t stereo_buf[32768] __attribute__((aligned(4)));

int ac97_play(const int16_t *data, int frames, int channels) {
    if (!ac97_ready || !data || frames <= 0) return -1;
    if (channels != 1 && channels != 2) return -1;

    /* Reset PCM Out channel */
    ac97_reset_channel(NABM_PCM_OUT);

    /* AC97 always outputs stereo. Convert mono→stereo if needed. */
    const int16_t *stereo_data;
    int total_samples;  /* total 16-bit words */

    if (channels == 1) {
        /* Play in chunks that fit stereo_buf (16384 frames at a time) */
        int played = 0;
        while (played < frames) {
            int chunk = frames - played;
            if (chunk > 16384) chunk = 16384;

            /* Mono → stereo: duplicate each sample */
            for (int i = 0; i < chunk; i++) {
                stereo_buf[i * 2]     = data[played + i];
                stereo_buf[i * 2 + 1] = data[played + i];
            }

            /* Play this stereo chunk */
            int ret = ac97_play(stereo_buf, chunk, 2);
            if (ret != 0) return ret;
            played += chunk;
        }
        return 0;
    }

    /* Stereo path — set up BDL entries */
    stereo_data = data;
    total_samples = frames * 2;  /* L + R per frame */

    int num_entries = 0;
    int remaining = total_samples;
    int offset = 0;

    while (remaining > 0 && num_entries < AC97_MAX_BDL) {
        int chunk = remaining > MAX_BDL_SAMPLES ? MAX_BDL_SAMPLES : remaining;
        out_bdl[num_entries].addr = (uint32_t)(uintptr_t)&stereo_data[offset];
        out_bdl[num_entries].samples = (uint16_t)chunk;
        out_bdl[num_entries].flags = (remaining - chunk <= 0) ? (BDL_IOC | BDL_BUP) : BDL_IOC;
        offset += chunk;
        remaining -= chunk;
        num_entries++;
    }

    if (num_entries == 0) return -1;

    /* Set BDL base address */
    nabm_write32(NABM_PCM_OUT + CH_BDBAR, (uint32_t)(uintptr_t)out_bdl);

    /* Set Last Valid Index */
    nabm_write8(NABM_PCM_OUT + CH_LVI, (uint8_t)(num_entries - 1));

    /* Clear status bits */
    nabm_write16(NABM_PCM_OUT + CH_SR, SR_LVBCI | SR_BCIS | SR_FIFOE);

    /* Start playback */
    nabm_write8(NABM_PCM_OUT + CH_CR, CR_RPBM | CR_LVBIE);

    /* Wait for Last Valid Buffer completion */
    u32_t start = sys_now();
    while (!(nabm_read16(NABM_PCM_OUT + CH_SR) & SR_LVBCI)) {
        if (sys_now() - start > 30000) {  /* 30s timeout */
            AC97_DBG("playback TIMEOUT\n");
            ac97_stop_playback();
            return -1;
        }
        __asm__ volatile("hlt");
    }

    /* Clear status and stop */
    nabm_write16(NABM_PCM_OUT + CH_SR, SR_LVBCI | SR_BCIS);
    nabm_write8(NABM_PCM_OUT + CH_CR, 0);

    return 0;
}

void ac97_stop_playback(void) {
    if (!ac97_ready) return;
    nabm_write8(NABM_PCM_OUT + CH_CR, 0);  /* Stop DMA */
    ac97_reset_channel(NABM_PCM_OUT);
}

/* ========================================================================= */
/* Capture                                                                   */
/* ========================================================================= */

int ac97_record(int16_t *buf, int frames) {
    if (!ac97_ready || !buf || frames <= 0) return -1;

    /* Reset PCM In channel */
    ac97_reset_channel(NABM_PCM_IN);

    /* Set up BDL for capture — stereo S16LE, 2 samples per frame */
    int total_samples = frames * 2;
    int num_entries = 0;
    int remaining = total_samples;
    int offset = 0;

    while (remaining > 0 && num_entries < AC97_MAX_BDL) {
        int chunk = remaining > MAX_BDL_SAMPLES ? MAX_BDL_SAMPLES : remaining;
        in_bdl[num_entries].addr = (uint32_t)(uintptr_t)&buf[offset];
        in_bdl[num_entries].samples = (uint16_t)chunk;
        in_bdl[num_entries].flags = (remaining - chunk <= 0) ? (BDL_IOC | BDL_BUP) : BDL_IOC;
        offset += chunk;
        remaining -= chunk;
        num_entries++;
    }

    if (num_entries == 0) return -1;

    /* Set BDL base address for PCM In */
    nabm_write32(NABM_PCM_IN + CH_BDBAR, (uint32_t)(uintptr_t)in_bdl);

    /* Set Last Valid Index */
    nabm_write8(NABM_PCM_IN + CH_LVI, (uint8_t)(num_entries - 1));

    /* Clear status */
    nabm_write16(NABM_PCM_IN + CH_SR, SR_LVBCI | SR_BCIS | SR_FIFOE);

    /* Start capture */
    nabm_write8(NABM_PCM_IN + CH_CR, CR_RPBM | CR_LVBIE);

    /* Wait for completion */
    u32_t start = sys_now();
    while (!(nabm_read16(NABM_PCM_IN + CH_SR) & SR_LVBCI)) {
        if (sys_now() - start > 30000) {
            AC97_DBG("capture TIMEOUT\n");
            ac97_stop_capture();
            return frames;  /* return what we got */
        }
        __asm__ volatile("hlt");
    }

    /* Clear status and stop */
    nabm_write16(NABM_PCM_IN + CH_SR, SR_LVBCI | SR_BCIS);
    nabm_write8(NABM_PCM_IN + CH_CR, 0);

    return frames;
}

void ac97_stop_capture(void) {
    if (!ac97_ready) return;
    nabm_write8(NABM_PCM_IN + CH_CR, 0);
    ac97_reset_channel(NABM_PCM_IN);
}
