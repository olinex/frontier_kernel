/* set archtecture as riscv */
OUTPUT_ARCH(riscv)

/* set entry point as symbol '_fn_start' which was defined in src/assembly/entry.asm */
ENTRY(_fn_start)

/* memory.ld will be generated by Makefile, will was choiced according to platform("sbi" and "board") */
INCLUDE memory.ld

SECTIONS
{
    /*  
        the first section was code text 
        declare the .text section distribution
    */
    .text : {
        PROVIDE(_addr_text_start = .);
        /* sub section .text.entry will be the first part of .text section */
        *(.text.entry)
        . = ALIGN(8k);
        /* sub section .text.trampoline */
        PROVIDE(_fn_trampoline = .);
        *(.text.trampoline)
        . = ALIGN(8k);
        /* other kernel code section .text */
        *(.text .text.*)
        PROVIDE(_addr_text_end = .);
    } > RAM

    /* declare inited read only data section */
    .rodata : {
        . = ALIGN(8k);
        PROVIDE(_addr_rodata_start = .);
        *(.rodata .rodata.*)
        PROVIDE(_addr_rodata_end = .);
    } > RAM

    /* declare inited read write data section */
    .data : {
        . = ALIGN(8k);
        PROVIDE(_addr_data_start = .);
        *(.data .data.*)
        PROVIDE(_addr_data_end = .);
    } > RAM

    /* declare uninit data section */
    .bss : {
        . = ALIGN(8k);
        /* sub section .bss.bootstack was defined in src/assemble/entry.asm  */
        PROVIDE(_addr_bootstack_start = .);
        *(.bss.bootstack)
        PROVIDE(_addr_bootstack_end = .);
        . = ALIGN(8k);
        PROVIDE(_addr_bss_start = .);
        *(.bss .bss.*)
        PROVIDE(_addr_bss_end = .);
    } > RAM

    PROVIDE(_addr_mem_start = ORIGIN(RAM));
    PROVIDE(_addr_mem_end = _addr_mem_start + LENGTH(RAM));
    PROVIDE(_addr_kernel_mem_start = _addr_mem_start);
    PROVIDE(_addr_kernel_mem_end = .);
    /* insert a guard page between kernel code and free memory */
    . = ALIGN(8k);
    PROVIDE(_addr_free_mem_start = .);
    PROVIDE(_addr_free_mem_end = _addr_mem_end);
    /DISCARD/ : {
        *(.eh_frame)
    }
}