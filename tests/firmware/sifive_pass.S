.section .text
.globl _start
_start:
    lui     a0, 0x100       # a0 = 0x100000
    li      a1, 0x5555      # a1 = PASS
    sw      a1, 0(a0)       # trigger shutdown
hang:
    wfi
    j       hang
