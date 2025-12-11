MEMORY {
    BOOT2   : ORIGIN = 0x10000000, LENGTH = 256
    FLASH   : ORIGIN = 0x10000100, LENGTH = 1500K - 256
	PROFILE : ORIGIN = 0x10180000, LENGTH = 500K
    RAM     : ORIGIN = 0x20000000, LENGTH = 256K
}

EXTERN(BOOT2_FIRMWARE)

SECTIONS {
    /* ### Boot loader */
    .boot2 ORIGIN(BOOT2) :
    {
        KEEP(*(.boot2));
    } > BOOT2
	/* profile data */
	.profile : {
		*(.profile);
	} > PROFILE
} INSERT BEFORE .text;