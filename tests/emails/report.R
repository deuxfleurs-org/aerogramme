library(tidyverse)
library(lubridate)
read_csv("imap_commands_summary.csv") -> cmd

ggplot(cmd, aes(x=command, y=count)) + 
  geom_bar(stat = "identity")+ 
  theme_classic() +
  facet_wrap(~aggregation, ncol=1, scales = "free")

read_csv("mailbox_email_sizes.csv") -> mbx
ggplot(mbx, aes(x=size, colour=mailbox)) + 
  stat_ecdf(pad=FALSE,geom = "step") +
scale_x_log10()+  theme_classic()
  
