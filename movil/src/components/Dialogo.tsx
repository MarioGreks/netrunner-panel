import type { ReactNode } from 'react';
import { KeyboardAvoidingView, Modal, Platform, Pressable, View } from 'react-native';
import { Marco } from './Marco';

interface Props {
  visible: boolean;
  titulo: string;
  onCerrar: () => void;
  /** Botones al pie, alineados a la derecha. */
  acciones?: ReactNode;
  children?: ReactNode;
}

/** Diálogo modal: Marco amarillo sobre velo negro. */
export function Dialogo({ visible, titulo, onCerrar, acciones, children }: Props) {
  return (
    <Modal visible={visible} transparent animationType="fade" onRequestClose={onCerrar}>
      <KeyboardAvoidingView behavior={Platform.OS === 'ios' ? 'padding' : undefined} style={{ flex: 1 }}>
        <Pressable
          onPress={onCerrar}
          style={{ flex: 1, backgroundColor: 'rgba(0,0,0,0.75)', justifyContent: 'center', padding: 24 }}
        >
          {/* el Pressable interior evita que tocar el diálogo lo cierre */}
          <Pressable>
            <Marco titulo={titulo}>
              {children}
              {acciones !== undefined && (
                <View style={{ flexDirection: 'row', justifyContent: 'flex-end', gap: 8, marginTop: 16 }}>{acciones}</View>
              )}
            </Marco>
          </Pressable>
        </Pressable>
      </KeyboardAvoidingView>
    </Modal>
  );
}
