global long_mode_start

section .text
bits 64
long_mode_start:
	;;  load 0 into all data segment registers
	mov ax, 0
	mov ss, ax
	mov ds, ax
	mov es, ax
	mov fs, ax
	mov gs, ax

	;;  print `Hej Johan :) ` to screen
	mov rax, 0x5f203f6a3f653f48
	mov qword [0xb8000], rax
	mov rax, 0x1f611f681f6f1f4a
	mov qword [0xb8008], rax
	mov rax, 0x5f295f3a5f201f6e
	mov qword [0xb8010], rax
	    hlt
