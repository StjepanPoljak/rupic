#[allow(non_camel_case_types)]
#[repr(u16)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Register {
    INDF = 0x00,
    TMR0 = 0x01,
    PCL = 0x02,
    STATUS = 0x03,
    FSR = 0x04,

    PORTA = 0x05,
    PORTB = 0x06,
    PORTC = 0x07,
    PORTD = 0x08,
    PORTE = 0x09,

    PCLATH = 0x0A,
    INTCON = 0x0B,

    PIR1 = 0x0C,
    PIR2 = 0x0D,

    TMR1L = 0x0E,
    TMR1H = 0x0F,
    T1CON = 0x10,

    TMR2 = 0x11,
    T2CON = 0x12,

    SSPBUF = 0x13,
    SSPCON = 0x14,
    SSPCON2 = 0x91,
    SSPADD = 0x93,
    SSPSTAT = 0x94,

    CCPR1L = 0x15,
    CCPR1H = 0x16,
    CCP1CON = 0x17,

    RCSTA = 0x18,
    TXREG = 0x19,
    RCREG = 0x1A,

    CCPR2L = 0x1B,
    CCPR2H = 0x1C,
    CCP2CON = 0x1D,

    ADRESH = 0x1E,
    ADCON0 = 0x1F,

    OPTION_REG = 0x81,

    TRISA = 0x85,
    TRISB = 0x86,
    TRISC = 0x87,
    TRISD = 0x88,
    TRISE = 0x89,

    PIE1 = 0x8C,
    PIE2 = 0x8D,

    PCON = 0x8E,

    TXSTA = 0x98,
    SPBRG = 0x99,

    ADRESL = 0x9E,
    ADCON1 = 0x9F,

    EEDATA = 0x10C,
    EEADR = 0x10D,
    EEDATH = 0x10E,
    EEADRH = 0x10F,

    EECON1 = 0x18C,
    EECON2 = 0x18D
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusBit {
    IRP = 0x7,
    RP1 = 0x6,
    RP0 = 0x5,
    TO = 0x4,
    PD = 0x3,
    Z = 0x2,
    DC = 0x1,
    C = 0x0
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DestinationBit {
    W = 0x0,
    F = 0x1
}
