/* set archtecture as riscv */
OUTPUT_ARCH(riscv)

/* set entry point as symbol '_start' which was defined in src/assemble/entry.asm */
ENTRY(_start)

/* set base address */
BASE_ADDRESS = 0x80200000;

SECTIONS
{
    /* rewrite current address to base address */
    . = BASE_ADDRESS;
    /* declare kernel start address */
    start_kernel = .;

    /*  the first section was code text */
    start_text = .;
    /* declare the .text section distribution */
    .text : {
        /* sub sction .text.entry will be the first part of .text section */
        *(.text.entry)
        *(.text .text.*)
    }

    /* declare inited read only data section */
    . = ALIGN(4K);
    end_text = .;
    start_rodata = .;
    .rodata : {
        *(.rodata .rodata.*)
        *(.start_rodata .start_rodata.*)
    }

    /* declare inited read write data section */
    . = ALIGN(4K);
    end_rodata = .;
    start_data = .;
    .data : {
        *(.data .data.*)
        *(.start_data .start_data.*)
        *(.data.bootstack)
    }

    /* declare uninit data section */
    . = ALIGN(4K);
    end_data = .;
    start_bss = .;
    .bss : {
        *(.bss .bss.*)
        *(.start_bss .start_bss.*)
    }

    . = ALIGN(4K);
    end_bss = .;
    end_kernel = .;

    /DISCARD/ : {
        *(.eh_frame)
    }
}