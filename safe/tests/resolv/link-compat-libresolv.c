extern unsigned int ns_get16 (const unsigned char *);

int
main (void)
{
  static const unsigned char value[] = { 0x12, 0x34 };
  return ns_get16 (value) != 0x1234;
}
