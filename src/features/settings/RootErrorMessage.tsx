import type { IpcError } from '../../platform/ipc';

export interface RootErrorCopy {
  title: string;
  message: string;
  actionLabel: string;
}

export function rootErrorCopy(error: IpcError): RootErrorCopy {
  switch (error.code) {
    case 'content_root_invalid_path':
      return {
        title: 'FOLDER PATH REJECTED',
        message:
          'Choose a folder from the native picker. Unsafe or unsupported path forms are not accepted.',
        actionLabel: 'CHOOSE ANOTHER FOLDER',
      };
    case 'content_root_unavailable':
      return {
        title: 'FOLDER UNAVAILABLE',
        message:
          'That folder cannot currently be reached. Check the drive or connection, then try again.',
        actionLabel: 'CHOOSE ANOTHER FOLDER',
      };
    case 'content_root_not_directory':
      return {
        title: 'NOT A FOLDER',
        message: 'The selected item is not a directory. Choose a folder that contains your ROMs.',
        actionLabel: 'CHOOSE ANOTHER FOLDER',
      };
    case 'content_root_overlap':
      return {
        title: 'FOLDER ALREADY COVERED',
        message:
          'This folder is already inside another enabled content root. Choose a separate folder.',
        actionLabel: 'CHOOSE ANOTHER FOLDER',
      };
    case 'content_root_invalid_operation':
      return {
        title: 'ROOT CANNOT BE CHANGED',
        message: 'The managed ROM folder is protected. Manage an external root instead.',
        actionLabel: 'MANAGE ROOTS',
      };
    default:
      return {
        title: 'FOLDER ACTION FAILED',
        message: 'RetroFrontier could not complete that folder action. Try again.',
        actionLabel: 'TRY AGAIN',
      };
  }
}
